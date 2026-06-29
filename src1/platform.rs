// src/platform.rs
use std::io;
use sysinfo::{CpuExt, System, SystemExt};

pub struct SystemEngine {
    sys: System,
}

impl SystemEngine {
    pub fn new() -> Self {
        let mut s = System::new_all();
        s.refresh_all();
        Self { sys: s }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_cpu();
        self.sys.refresh_memory();
    }

    pub fn get_metrics(&self) -> (f64, f64, f64, f64) {
        let free_ram = self.sys.free_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let cpu_load = self.sys.global_cpu_info().cpu_usage() as f64 / 100.0;
        let total_swap = self.sys.total_swap() as f64;
        let used_swap = self.sys.used_swap() as f64 / 1024.0 / 1024.0 / 1024.0;
        (free_ram, cpu_load, total_swap, used_swap)
    }

    pub fn get_full_sys(&mut self) -> &mut System {
        &mut self.sys
    }
}

pub struct Interface {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
}

impl Interface {
    pub fn init() -> Result<Self, io::Error> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = ratatui::Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    // Esponiamo il frame in modo che ui.rs possa usarlo senza che main lo importi
    pub fn draw<F>(&mut self, f: F) -> io::Result<()> 
    where F: FnOnce(&mut ratatui::Frame<ratatui::backend::CrosstermBackend<io::Stdout>>) {
        self.terminal.draw(f)?;
        Ok(())
    }

    pub fn cleanup(&mut self) -> Result<(), io::Error> {
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(self.terminal.backend_mut(), crossterm::terminal::LeaveAlternateScreen)?;
        Ok(())
    }

    pub fn poll_quit(&self) -> bool {
        use crossterm::event::{self, Event, KeyCode};
        if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                return key.code == KeyCode::Esc || key.code == KeyCode::Char('q');
            }
        }
        false
    }
}
