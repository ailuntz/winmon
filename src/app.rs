use crate::config::{Config, ViewType};
use crate::metrics::{MemMetrics, Metrics, zero_div};
use crate::sources::{DeviceInfo, Sampler, WithError, load_device_info};
use crossterm::{
    ExecutableCommand,
    event::{self, KeyCode, KeyModifiers},
    terminal,
};
use ratatui::{backend::CrosstermBackend, prelude::*, widgets::*};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const GB: u64 = 1024 * 1024 * 1024;
const MAX_SPARKLINE: usize = 128;
const MAX_TEMPS: usize = 8;
const MAX_REASONABLE_TEMP_C: f32 = 150.0;
const MAX_TEMP_DELTA_C: f32 = 20.0;

fn enter_term() -> Terminal<CrosstermBackend<std::io::Stdout>> {
    std::panic::set_hook(Box::new(|info| {
        leave_term();
        eprintln!("{info}");
    }));

    terminal::enable_raw_mode().unwrap();
    std::io::stdout()
        .execute(terminal::EnterAlternateScreen)
        .unwrap();
    let backend = CrosstermBackend::new(std::io::stdout());
    Terminal::new(backend).unwrap()
}

fn leave_term() {
    terminal::disable_raw_mode().unwrap();
    std::io::stdout()
        .execute(terminal::LeaveAlternateScreen)
        .unwrap();
}

#[derive(Debug, Default)]
struct UsageStore {
    items: Vec<u64>,
    top_value: u64,
    usage: f64,
    available: bool,
}

impl UsageStore {
    fn push(&mut self, value: u64, usage: f64, available: bool) {
        self.items.insert(0, (usage * 100.0) as u64);
        self.items.truncate(MAX_SPARKLINE);
        self.top_value = value;
        self.usage = usage;
        self.available = available;
    }
}

#[derive(Debug, Default)]
struct PowerStore {
    items: Vec<u64>,
    top_value: f64,
    max_value: f64,
    avg_value: f64,
    available: bool,
}

impl PowerStore {
    fn push(&mut self, value: Option<f32>) {
        let Some(value) = value else {
            self.items.insert(0, 0);
            self.items.truncate(MAX_SPARKLINE);
            return;
        };

        let value = value as f64;
        let was_top = if !self.items.is_empty() {
            self.items[0] as f64 / 1000.0
        } else {
            0.0
        };
        self.items.insert(0, (value * 1000.0) as u64);
        self.items.truncate(MAX_SPARKLINE);
        self.top_value = avg2(was_top, value);
        self.avg_value = self.items.iter().sum::<u64>() as f64 / self.items.len() as f64 / 1000.0;
        self.max_value = self.items.iter().copied().max().unwrap_or_default() as f64 / 1000.0;
        self.available = true;
    }
}

#[derive(Debug, Default)]
struct MemoryStore {
    items: Vec<u64>,
    ram_usage: u64,
    ram_total: u64,
    swap_usage: u64,
    swap_total: u64,
}

impl MemoryStore {
    fn push(&mut self, value: MemMetrics) {
        self.items.insert(0, value.ram_usage);
        self.items.truncate(MAX_SPARKLINE);
        self.ram_usage = value.ram_usage;
        self.ram_total = value.ram_total;
        self.swap_usage = value.swap_usage;
        self.swap_total = value.swap_total;
    }
}

#[derive(Debug, Default)]
struct TempStore {
    items: Vec<f32>,
}

impl TempStore {
    fn last(&self) -> Option<f32> {
        self.items.first().copied()
    }

    fn push(&mut self, value: Option<f32>) {
        let mut value = match value {
            Some(value) if value.is_finite() && value > 0.0 && value <= MAX_REASONABLE_TEMP_C => {
                value
            }
            _ => match self.trend_ema(0.8) {
                Some(value) => value,
                None => return,
            },
        };

        if let Some(last) = self.last() {
            let delta = value - last;
            if delta.abs() > MAX_TEMP_DELTA_C {
                value = last + delta.signum() * (MAX_TEMP_DELTA_C * 0.35);
            } else {
                value = last * 0.35 + value * 0.65;
            }
        }

        self.items.insert(0, value.max(0.0));
        self.items.truncate(MAX_TEMPS);
    }

    fn trend_ema(&self, alpha: f32) -> Option<f32> {
        if self.items.is_empty() {
            return None;
        }

        let mut iter = self.items.iter().rev();
        let mut ema = *iter.next()?;
        for &item in iter {
            ema = alpha * item + (1.0 - alpha) * ema;
        }
        Some(ema)
    }
}

fn h_stack(area: Rect) -> (Rect, Rect) {
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Fill(1)])
        .split(area);
    (parts[0], parts[1])
}

enum Event {
    Update(Metrics),
    SamplerError(String),
    ChangeColor,
    ChangeView,
    IncInterval,
    DecInterval,
    Tick,
    Quit,
}

fn handle_key_event(key: &event::KeyEvent, tx: &mpsc::Sender<Event>) -> WithError<()> {
    match key.code {
        KeyCode::Char('q') => Ok(tx.send(Event::Quit)?),
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => Ok(tx.send(Event::Quit)?),
        KeyCode::Char('c') => Ok(tx.send(Event::ChangeColor)?),
        KeyCode::Char('v') => Ok(tx.send(Event::ChangeView)?),
        KeyCode::Char('+') | KeyCode::Char('=') => Ok(tx.send(Event::IncInterval)?),
        KeyCode::Char('-') => Ok(tx.send(Event::DecInterval)?),
        _ => Ok(()),
    }
}

fn run_inputs_thread(tx: mpsc::Sender<Event>, tick: u64) {
    let tick_rate = Duration::from_millis(tick);

    std::thread::spawn(move || {
        let mut last_tick = Instant::now();

        loop {
            if event::poll(Duration::from_millis(tick)).unwrap() {
                if let event::Event::Key(key) = event::read().unwrap() {
                    handle_key_event(&key, &tx).unwrap();
                }
            }

            if last_tick.elapsed() >= tick_rate {
                tx.send(Event::Tick).unwrap();
                last_tick = Instant::now();
            }
        }
    });
}

fn run_sampler_thread(
    tx: mpsc::Sender<Event>,
    msec: Arc<RwLock<u32>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let Ok(mut sampler) = Sampler::new() else {
            let _ = tx.send(Event::SamplerError("sampler init failed".to_string()));
            return;
        };

        while !stop.load(Ordering::Relaxed) {
            let interval = (*msec.read().unwrap()).max(500) as u64;
            let started = Instant::now();
            match sampler.get_metrics() {
                Ok(metrics) => {
                    if tx.send(Event::Update(metrics)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Event::SamplerError(err.to_string()));
                }
            }
            let elapsed = started.elapsed();
            let target = Duration::from_millis(interval);
            let mut remaining = target.saturating_sub(elapsed);
            while remaining > Duration::ZERO && !stop.load(Ordering::Relaxed) {
                let chunk = remaining.min(Duration::from_millis(100));
                std::thread::sleep(chunk);
                remaining = remaining.saturating_sub(chunk);
            }
        }
    })
}

fn avg2(a: f64, b: f64) -> f64 {
    if a == 0.0 { b } else { (a + b) / 2.0 }
}

#[derive(Debug, Default)]
pub struct App {
    cfg: Config,
    device: DeviceInfo,
    mem: MemoryStore,
    cpu_power: PowerStore,
    gpu_power: PowerStore,
    sys_power: PowerStore,
    cpu_temp: TempStore,
    gpu_temp: TempStore,
    e_cpu_usage: UsageStore,
    p_cpu_usage: UsageStore,
    gpu_usage: UsageStore,
    last_error: Option<String>,
}

impl App {
    pub fn new() -> WithError<Self> {
        let cfg = Config::load();
        let device = load_device_info()?;
        Ok(Self {
            cfg,
            device,
            ..Default::default()
        })
    }

    fn update_metrics(&mut self, data: Metrics) {
        self.last_error = None;
        self.e_cpu_usage.push(
            data.e_cpu_usage.0 as u64,
            data.e_cpu_usage.1 as f64,
            data.e_cpu_usage.0 > 0 || data.e_cpu_usage.1 > 0.0,
        );
        self.p_cpu_usage.push(
            data.p_cpu_usage.0 as u64,
            data.p_cpu_usage.1 as f64,
            data.p_cpu_usage.0 > 0 || data.p_cpu_usage.1 > 0.0,
        );
        self.gpu_usage.push(
            data.gpu_usage.0 as u64,
            data.gpu_usage.1 as f64,
            self.device.gpu_backend != "none",
        );
        self.cpu_temp.push(data.temp.cpu_temp);
        self.gpu_temp.push(data.temp.gpu_temp);
        self.cpu_power.push(data.power.cpu_power);
        self.gpu_power.push(data.power.gpu_power);
        self.sys_power.push(data.power.sys_power);
        self.mem.push(data.memory);
    }

    fn title_block<'a>(&self, label_l: &str, label_r: &str) -> Block<'a> {
        let mut block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(self.cfg.color)
            .padding(Padding::ZERO);

        if !label_l.is_empty() {
            block = block.title_top(Line::from(format!(" {label_l} ")));
        }

        if !label_r.is_empty() {
            block = block.title_top(Line::from(format!(" {label_r} ")).alignment(Alignment::Right));
        }

        block
    }

    fn render_usage_block(&self, f: &mut Frame, area: Rect, label: &str, store: &UsageStore) {
        let label = if !store.available {
            format!("{label} N/A")
        } else if store.top_value > 0 {
            format!(
                "{label} {:3.0}% @ {:4} MHz",
                store.usage * 100.0,
                store.top_value
            )
        } else {
            format!("{label} {:3.0}%", store.usage * 100.0)
        };
        let block = self.title_block(&label, "");

        match self.cfg.view_type {
            ViewType::Sparkline => {
                let widget = Sparkline::default()
                    .block(block)
                    .direction(RenderDirection::RightToLeft)
                    .data(&store.items)
                    .max(100)
                    .style(self.cfg.color);
                f.render_widget(widget, area);
            }
            ViewType::Gauge => {
                let widget = Gauge::default()
                    .block(block)
                    .gauge_style(self.cfg.color)
                    .style(self.cfg.color)
                    .label("")
                    .ratio(store.usage);
                f.render_widget(widget, area);
            }
        }
    }

    fn render_mem_block(&self, f: &mut Frame, area: Rect) {
        let ram_usage_gb = self.mem.ram_usage as f64 / GB as f64;
        let ram_total_gb = self.mem.ram_total as f64 / GB as f64;
        let swap_usage_gb = self.mem.swap_usage as f64 / GB as f64;
        let swap_total_gb = self.mem.swap_total as f64 / GB as f64;
        let ram_usage_pct = zero_div(ram_usage_gb, ram_total_gb.max(0.0001)) * 100.0;

        let label_l = format!(
            "RAM {:4.2} / {:4.1} GB ({:.1}%)",
            ram_usage_gb, ram_total_gb, ram_usage_pct
        );
        let label_r = format!("SWAP {:.2} / {:.1} GB", swap_usage_gb, swap_total_gb);
        let block = self.title_block(&label_l, &label_r);

        match self.cfg.view_type {
            ViewType::Sparkline => {
                let widget = Sparkline::default()
                    .block(block)
                    .direction(RenderDirection::RightToLeft)
                    .data(&self.mem.items)
                    .max(self.mem.ram_total.max(1))
                    .style(self.cfg.color);
                f.render_widget(widget, area);
            }
            ViewType::Gauge => {
                let widget = Gauge::default()
                    .block(block)
                    .gauge_style(self.cfg.color)
                    .style(self.cfg.color)
                    .label("")
                    .ratio(zero_div(ram_usage_gb, ram_total_gb.max(0.0001)));
                f.render_widget(widget, area);
            }
        }
    }

    fn power_title(&self, label: &str, store: &PowerStore, temp: Option<f32>) -> (String, String) {
        let left = if store.available {
            format!(
                "{label} {:.2}W ({:.2}, {:.2})",
                store.top_value, store.avg_value, store.max_value
            )
        } else {
            format!("{label} N/A")
        };

        let right = match temp {
            Some(temp) => format!("{temp:.1}°C"),
            None => "N/A".to_string(),
        };

        (left, right)
    }

    fn render_power_block(
        &self,
        f: &mut Frame,
        area: Rect,
        label: &str,
        store: &PowerStore,
        temp: Option<f32>,
    ) {
        let (label_l, label_r) = self.power_title(label, store, temp);
        let widget = Sparkline::default()
            .block(self.title_block(&label_l, &label_r))
            .direction(RenderDirection::RightToLeft)
            .data(&store.items)
            .style(self.cfg.color);
        f.render_widget(widget, area);
    }

    fn render(&mut self, f: &mut Frame) {
        let title = format!(
            "{} | {}C/{}T | {}",
            self.device.machine_name,
            self.device.cpu_cores,
            self.device.cpu_threads,
            self.device.gpu_name
        );

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(2), Constraint::Fill(1)])
            .split(f.area());

        let brand = format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        let block = self.title_block(&title, &brand);
        let inner = block.inner(rows[0]);
        f.render_widget(block, rows[0]);

        let inner_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Fill(1)])
            .split(inner);

        let (r1c1, r1c2) = h_stack(inner_rows[0]);
        self.render_usage_block(f, r1c1, "E-CPU", &self.e_cpu_usage);
        self.render_usage_block(f, r1c2, "P-CPU", &self.p_cpu_usage);

        let (r2c1, r2c2) = h_stack(inner_rows[1]);
        self.render_mem_block(f, r2c1);
        self.render_usage_block(f, r2c2, "GPU", &self.gpu_usage);

        let power_left = if self.cpu_power.available || self.gpu_power.available {
            format!(
                "Power {:.2}W (avg {:.2}W, max {:.2}W)",
                self.cpu_power.top_value + self.gpu_power.top_value,
                self.cpu_power.avg_value + self.gpu_power.avg_value,
                self.cpu_power.max_value + self.gpu_power.max_value
            )
        } else {
            "Power".to_string()
        };

        let power_right = if self.sys_power.available {
            format!(
                "System {:.2}W ({:.2}, {:.2})",
                self.sys_power.top_value, self.sys_power.avg_value, self.sys_power.max_value
            )
        } else {
            self.last_error
                .as_ref()
                .map(|err| format!("Error {err}"))
                .unwrap_or_default()
        };

        let block = self.title_block(&power_left, &power_right);
        let usage = format!(" q quit | c color | v view | -/+ {}ms ", self.cfg.interval);
        let block = block.title_bottom(Line::from(usage).right_aligned());
        let inner = block.inner(rows[1]);
        f.render_widget(block, rows[1]);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ])
            .split(inner);

        self.render_power_block(f, cols[0], "CPU", &self.cpu_power, self.cpu_temp.last());
        self.render_power_block(f, cols[1], "GPU", &self.gpu_power, self.gpu_temp.last());
        self.render_power_block(f, cols[2], "SYS", &self.sys_power, None);
    }

    pub fn run_loop(&mut self, interval: Option<u32>) -> WithError<()> {
        self.cfg.interval = interval.unwrap_or(self.cfg.interval).clamp(500, 10_000);
        let msec = Arc::new(RwLock::new(self.cfg.interval));
        let stop = Arc::new(AtomicBool::new(false));

        let (tx, rx) = mpsc::channel::<Event>();
        run_inputs_thread(tx.clone(), 250);
        let sampler = run_sampler_thread(tx, Arc::clone(&msec), Arc::clone(&stop));

        let mut term = enter_term();

        loop {
            term.draw(|f| self.render(f))?;

            match rx.recv()? {
                Event::Quit => break,
                Event::Update(data) => self.update_metrics(data),
                Event::SamplerError(err) => self.last_error = Some(err),
                Event::ChangeColor => self.cfg.next_color(),
                Event::ChangeView => self.cfg.next_view_type(),
                Event::IncInterval => {
                    self.cfg.inc_interval();
                    *msec.write().unwrap() = self.cfg.interval;
                }
                Event::DecInterval => {
                    self.cfg.dec_interval();
                    *msec.write().unwrap() = self.cfg.interval;
                }
                Event::Tick => {}
            }
        }

        stop.store(true, Ordering::Relaxed);
        let _ = sampler.join();
        leave_term();
        Ok(())
    }
}
