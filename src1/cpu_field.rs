use std::fs::File;
use std::io::{Read, BufReader};
use crate::fp::{eval}; // Usiamo le nostre monadi SAPRI

/// Struttura per mappare la CPU nel Campo Morfogenetico
pub struct CpuContext {
    pub total_prev: u64,
    pub idle_prev: u64,
}

impl CpuContext {
    /// Legge i dati grezzi da Ubuntu senza allocazioni pesanti
    pub fn read_stat() -> Option<(u64, u64)> {
        let file = File::open("/proc/stat").ok()?;
        let mut reader = BufReader::new(file);
        let mut buffer = String::with_capacity(256);
        reader.read_to_string(&mut buffer).ok()?;

        // SAPRI Style: Chaining e Pattern Matching invece di if/else
        buffer.lines().next().and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let user: u64 = parts.get(1)?.parse().ok()?;
            let nice: u64 = parts.get(2)?.parse().ok()?;
            let system: u64 = parts.get(3)?.parse().ok()?;
            let idle: u64 = parts.get(4)?.parse().ok()?;
            let iowait: u64 = parts.get(5)?.parse().ok()?;
            let irq: u64 = parts.get(6)?.parse().ok()?;
            let softirq: u64 = parts.get(7)?.parse().ok()?;

            let total = user + nice + system + idle + iowait + irq + softirq;
            Some((total, idle))
        })
    }

    /// Calcola la risonanza della CPU (v)
    pub fn get_load_v(prev: &mut CpuContext) -> f64 {
        let (total, idle) = Self::read_stat().unwrap_or((0, 0));
        let diff_total = total - prev.total_prev;
        let diff_idle = idle - prev.idle_prev;

        // Aggiorna lo stato precedente (Inerzia del Campo)
        prev.total_prev = total;
        prev.idle_prev = idle;

        eval(diff_total == 0, 
            || 0.0, 
            || (diff_total - diff_idle) as f64 / diff_total as f64
        )
    }
}
