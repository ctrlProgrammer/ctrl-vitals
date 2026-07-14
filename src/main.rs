mod monitor;
mod mcp;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box, DrawingArea, Grid, Label, Orientation, ProgressBar,
};

const HISTORY_LEN: usize = 60;
const POLL_INTERVAL: u32 = 2;

struct AppState {
    prev_snapshots: Vec<monitor::CpuSnapshot>,
    history: monitor::UsageHistory,
    gpu_history: monitor::UsageHistory,
    ram_history: monitor::UsageHistory,
    max_avg: f64,
    sum_avg: f64,
    sample_count: u32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("mcp") {
        eprintln!("ctrl-vitals: starting MCP server");
        if let Err(e) = mcp::run() {
            eprintln!("ctrl-vitals: MCP server error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    let app = Application::new(Some("com.ctrl-vitals.monitor"), Default::default());
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let sensor = monitor::detect_temp_sensor();
    if sensor.is_some() {
        eprintln!("ctrl-vitals: using sensor at k10temp");
    } else {
        eprintln!("ctrl-vitals: no CPU temperature sensor found");
    }

    let initial = monitor::read_all_cpu_snapshots().unwrap_or_default();
    let core_count = if initial.len() > 1 {
        initial.len() - 1
    } else {
        0
    };
    eprintln!("ctrl-vitals: detected {} CPU cores", core_count);

    let state = Rc::new(RefCell::new(AppState {
        prev_snapshots: initial,
        history: monitor::UsageHistory::new(HISTORY_LEN),
        gpu_history: monitor::UsageHistory::new(HISTORY_LEN),
        ram_history: monitor::UsageHistory::new(HISTORY_LEN),
        max_avg: 0.0,
        sum_avg: 0.0,
        sample_count: 0,
    }));

    let iface = monitor::detect_active_interface()
        .unwrap_or_else(|| "eth0".to_string());
    eprintln!("ctrl-vitals: network interface: {}", iface);
    let net_monitor = Rc::new(RefCell::new(monitor::NetMonitor::new(iface)));

    let header = Label::new(Some("CPU Performance"));
    header.add_css_class("header");
    header.set_halign(Align::Start);

    let temp_label = Label::new(Some("--°"));
    temp_label.add_css_class("temp");
    temp_label.set_halign(Align::Center);

    let sparkline = DrawingArea::new();
    sparkline.set_hexpand(true);
    sparkline.set_content_height(48);
    sparkline.set_size_request(-1, 40);

    let progress_bar = ProgressBar::new();
    progress_bar.set_show_text(false);
    progress_bar.set_hexpand(true);
    progress_bar.set_valign(Align::Center);
    progress_bar.add_css_class("usage-bar");

    let usage_label = Label::new(Some("--%"));
    usage_label.add_css_class("usage-value");
    usage_label.set_halign(Align::End);

    let max_label = Label::new(Some("Max --%"));
    max_label.add_css_class("stat");
    let avg_label = Label::new(Some("Avg --%"));
    avg_label.add_css_class("stat");

    // --- Sparkline draw ---
    let state_draw = state.clone();
    sparkline.set_draw_func(move |_area, cr, width, height| {
        let st = state_draw.borrow();
        if st.history.len() < 2 || width < 10 || height < 5 {
            return;
        }
        let w = width as f64;
        let h = height as f64;
        let max_val = st.history.max();
        let min_val = st.history.min();
        let range = (max_val - min_val).max(5.0);
        let count = st.history.len();
        let hist = st.history.as_slice();

        cr.move_to(0.0, h);
        for (i, &val) in hist.iter().enumerate() {
            let x = i as f64 * w / (count - 1) as f64;
            let y = h - ((val - min_val) / range) * (h - 4.0) - 2.0;
            cr.line_to(x, y);
        }
        cr.line_to(w, h);
        cr.close_path();
        cr.set_source_rgba(1.0, 0.6, 0.1, 0.12);
        cr.fill().ok();

        cr.move_to(0.0, h);
        for (i, &val) in hist.iter().enumerate() {
            let x = i as f64 * w / (count - 1) as f64;
            let y = h - ((val - min_val) / range) * (h - 4.0) - 2.0;
            cr.line_to(x, y);
        }
        cr.set_source_rgba(1.0, 0.6, 0.1, 0.85);
        cr.set_line_width(1.5);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.stroke().ok();
    });

    // --- Per-core grid ---
    let core_grid = Grid::new();
    core_grid.set_column_spacing(6);
    core_grid.set_row_spacing(3);
    core_grid.set_halign(Align::Fill);
    core_grid.set_hexpand(true);

    let mut core_widgets: Vec<(ProgressBar, Label)> = Vec::with_capacity(core_count);
    let cols = 2;
    for i in 0..core_count {
        let c_label = Label::new(Some(&format!("C{}", i)));
        c_label.add_css_class("core-label");
        c_label.set_xalign(0.0);

        let c_bar = ProgressBar::new();
        c_bar.set_show_text(false);
        c_bar.set_hexpand(true);
        c_bar.set_valign(Align::Center);
        c_bar.add_css_class("core-bar");

        let c_pct = Label::new(Some("--%"));
        c_pct.add_css_class("core-pct");
        c_pct.set_xalign(1.0);

        let cell = Box::new(Orientation::Horizontal, 4);
        cell.append(&c_label);
        cell.append(&c_bar);
        cell.append(&c_pct);

        let row = (i / cols) as i32;
        let col = (i % cols) as i32;
        core_grid.attach(&cell, col, row, 1, 1);

        core_widgets.push((c_bar, c_pct));
    }

    // --- Layout ---
    let widget = Box::new(Orientation::Vertical, 5);
    widget.add_css_class("widget");

    widget.append(&header);
    widget.append(&temp_label);
    widget.append(&sparkline);

    let bar_row = Box::new(Orientation::Horizontal, 8);
    bar_row.set_halign(Align::Fill);
    bar_row.append(&progress_bar);
    bar_row.append(&usage_label);
    widget.append(&bar_row);

    if core_count > 0 {
        let sep = Label::new(Some("CORES"));
        sep.add_css_class("section-header");
        widget.append(&sep);
        widget.append(&core_grid);
    }

    // --- GPU ---
    let gpu_sep = Label::new(Some("GPU"));
    gpu_sep.add_css_class("section-header");

    let gpu_sparkline = DrawingArea::new();
    gpu_sparkline.set_hexpand(true);
    gpu_sparkline.set_content_height(36);
    gpu_sparkline.set_size_request(-1, 28);

    let state_gpu = state.clone();
    gpu_sparkline.set_draw_func(move |_area, cr, width, height| {
        let st = state_gpu.borrow();
        if st.gpu_history.len() < 2 || width < 10 || height < 5 {
            return;
        }
        let w = width as f64;
        let h = height as f64;
        let max_val = st.gpu_history.max();
        let min_val = st.gpu_history.min();
        let range = (max_val - min_val).max(5.0);
        let count = st.gpu_history.len();
        let hist = st.gpu_history.as_slice();

        cr.move_to(0.0, h);
        for (i, &val) in hist.iter().enumerate() {
            let x = i as f64 * w / (count - 1) as f64;
            let y = h - ((val - min_val) / range) * (h - 4.0) - 2.0;
            cr.line_to(x, y);
        }
        cr.line_to(w, h);
        cr.close_path();
        cr.set_source_rgba(0.39, 0.82, 1.0, 0.10);
        cr.fill().ok();

        cr.move_to(0.0, h);
        for (i, &val) in hist.iter().enumerate() {
            let x = i as f64 * w / (count - 1) as f64;
            let y = h - ((val - min_val) / range) * (h - 4.0) - 2.0;
            cr.line_to(x, y);
        }
        cr.set_source_rgba(0.39, 0.82, 1.0, 0.80);
        cr.set_line_width(1.5);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.stroke().ok();
    });

    let gpu_bar = ProgressBar::new();
    gpu_bar.set_show_text(false);
    gpu_bar.set_hexpand(true);
    gpu_bar.set_valign(Align::Center);
    gpu_bar.add_css_class("gpu-bar");

    let gpu_usage_label = Label::new(Some("--%"));
    gpu_usage_label.add_css_class("gpu-usage");

    let gpu_temp_label = Label::new(Some("--°"));
    gpu_temp_label.add_css_class("gpu-temp");

    let vram_bar = ProgressBar::new();
    vram_bar.set_show_text(false);
    vram_bar.set_hexpand(true);
    vram_bar.set_valign(Align::Center);
    vram_bar.add_css_class("vram-bar");

    let vram_label = Label::new(Some("-- / -- GB"));
    vram_label.add_css_class("vram-label");

    let gpu_row = Box::new(Orientation::Horizontal, 6);
    gpu_row.set_halign(Align::Fill);
    gpu_row.append(&gpu_bar);
    gpu_row.append(&gpu_usage_label);
    gpu_row.append(&gpu_temp_label);

    let vram_row = Box::new(Orientation::Horizontal, 6);
    vram_row.set_halign(Align::Fill);
    vram_row.append(&vram_bar);
    vram_row.append(&vram_label);

    widget.append(&gpu_sep);
    widget.append(&gpu_sparkline);
    widget.append(&gpu_row);
    widget.append(&vram_row);

    // --- RAM ---
    let ram_sep = Label::new(Some("RAM"));
    ram_sep.add_css_class("section-header");

    let ram_sparkline = DrawingArea::new();
    ram_sparkline.set_hexpand(true);
    ram_sparkline.set_content_height(28);
    ram_sparkline.set_size_request(-1, 24);

    let state_ram = state.clone();
    ram_sparkline.set_draw_func(move |_area, cr, width, height| {
        let st = state_ram.borrow();
        if st.ram_history.len() < 2 || width < 10 || height < 5 {
            return;
        }
        let w = width as f64;
        let h = height as f64;
        let max_val = st.ram_history.max();
        let min_val = st.ram_history.min();
        let range = (max_val - min_val).max(5.0);
        let count = st.ram_history.len();
        let hist = st.ram_history.as_slice();
        cr.move_to(0.0, h);
        for (i, &val) in hist.iter().enumerate() {
            let x = i as f64 * w / (count - 1) as f64;
            let y = h - ((val - min_val) / range) * (h - 4.0) - 2.0;
            cr.line_to(x, y);
        }
        cr.line_to(w, h);
        cr.close_path();
        cr.set_source_rgba(0.66, 0.33, 0.97, 0.10);
        cr.fill().ok();
        cr.move_to(0.0, h);
        for (i, &val) in hist.iter().enumerate() {
            let x = i as f64 * w / (count - 1) as f64;
            let y = h - ((val - min_val) / range) * (h - 4.0) - 2.0;
            cr.line_to(x, y);
        }
        cr.set_source_rgba(0.66, 0.33, 0.97, 0.80);
        cr.set_line_width(1.5);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.stroke().ok();
    });

    let ram_bar = ProgressBar::new();
    ram_bar.set_show_text(false);
    ram_bar.set_hexpand(true);
    ram_bar.set_valign(Align::Center);
    ram_bar.add_css_class("ram-bar");

    let ram_label = Label::new(Some("-- / -- GB"));
    ram_label.add_css_class("ram-label");

    let ram_row = Box::new(Orientation::Horizontal, 6);
    ram_row.set_halign(Align::Fill);
    ram_row.append(&ram_bar);
    ram_row.append(&ram_label);
    widget.append(&ram_sep);
    widget.append(&ram_sparkline);
    widget.append(&ram_row);

    // --- Disks ---
    let disk_sep = Label::new(Some("DISKS"));
    disk_sep.add_css_class("section-header");

    let disk_bar = ProgressBar::new();
    disk_bar.set_show_text(false);
    disk_bar.set_hexpand(true);
    disk_bar.set_valign(Align::Center);
    disk_bar.add_css_class("disk-bar");

    let disk_label = Label::new(Some("-- / -- GB"));
    disk_label.add_css_class("disk-label");

    let disk_row = Box::new(Orientation::Horizontal, 6);
    disk_row.set_halign(Align::Fill);
    disk_row.append(&disk_bar);
    disk_row.append(&disk_label);
    widget.append(&disk_sep);
    widget.append(&disk_row);

    // --- Processes ---
    let proc_sep = Label::new(Some("PROCESSES"));
    proc_sep.add_css_class("section-header");
    widget.append(&proc_sep);

    struct ProcRow {
        name: Label,
        cpu: Label,
        mem: Label,
    }
    let mut proc_rows: Vec<ProcRow> = Vec::new();
    for _ in 0..4 {
        let name = Label::new(Some(""));
        name.add_css_class("proc-name");
        let cpu = Label::new(Some(""));
        cpu.add_css_class("proc-cpu");
        let mem = Label::new(Some(""));
        mem.add_css_class("proc-mem");
        let cell = Box::new(Orientation::Horizontal, 8);
        cell.set_halign(Align::Center);
        cell.append(&name);
        cell.append(&cpu);
        cell.append(&mem);
        widget.append(&cell);
        proc_rows.push(ProcRow { name, cpu, mem });
    }

    // --- Network ---
    let net_sep = Label::new(Some("NETWORK"));
    net_sep.add_css_class("section-header");
    widget.append(&net_sep);

    let net_label = Label::new(Some("▼ -- KB/s  ▲ -- KB/s"));
    net_label.add_css_class("net-label");
    widget.append(&net_label);

    let stats_row = Box::new(Orientation::Horizontal, 16);
    stats_row.set_halign(Align::Center);
    stats_row.append(&max_label);
    stats_row.append(&avg_label);
    widget.append(&stats_row);

    // --- Window ---
    let window = ApplicationWindow::new(app);
    window.set_child(Some(&widget));
    window.set_default_size(290, 530);
    window.set_decorated(false);

    // --- Draggable ---
    use gtk4::gdk::prelude::ToplevelExt;
    let drag = gtk4::GestureDrag::new();
    let win = window.clone();
    drag.connect_drag_begin(move |gesture, x, y| {
        if let Some(surface) = win.surface() {
            if let Some(device) = gesture.device() {
                let time = gesture
                    .last_event(None)
                    .as_ref()
                    .map(|e| e.time())
                    .unwrap_or(0);
                if let Ok(toplevel) = surface.downcast::<gtk4::gdk::Toplevel>() {
                    toplevel.begin_move(&device, 1, x, y, time);
                }
            }
        }
    });
    widget.add_controller(drag);

    // --- CSS ---
    let css = r#"
window { background: transparent; }
.widget {
  background: rgba(28, 28, 34, 0.94);
  border-radius: 20px;
  padding: 18px 16px;
  margin: 8px;
}
.header {
  font-size: 13px;
  font-weight: 700;
  color: rgba(255, 255, 255, 0.50);
}
.temp {
  font-size: 46px;
  font-weight: 200;
  color: rgba(255, 255, 255, 0.90);
  padding: 2px 0;
}
progressbar.usage-bar { min-height: 5px; }
progressbar.usage-bar trough {
  min-height: 5px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.08);
  border: none;
  box-shadow: none;
}
progressbar.usage-bar progress {
  min-height: 5px;
  border-radius: 3px;
  background: #ff9f0a;
}
.usage-value {
  font-size: 14px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.70);
}
.section-header {
  font-size: 9px;
  font-weight: 600;
  color: rgba(255, 255, 255, 0.30);
  letter-spacing: 1.5px;
  margin-top: 4px;
}
.core-label {
  font-size: 10px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.50);
  min-width: 20px;
}
.core-pct {
  font-size: 10px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.40);
  min-width: 26px;
}
progressbar.core-bar { min-height: 4px; }
progressbar.core-bar trough {
  min-height: 4px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.06);
  border: none;
  box-shadow: none;
}
progressbar.core-bar progress {
  min-height: 4px;
  border-radius: 2px;
  background: #ff9f0a;
}
.gpu-usage {
  font-size: 13px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.70);
}
.gpu-temp {
  font-size: 13px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.55);
  min-width: 30px;
}
progressbar.gpu-bar { min-height: 6px; }
progressbar.gpu-bar trough {
  min-height: 6px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.08);
  border: none;
  box-shadow: none;
}
progressbar.gpu-bar progress {
  min-height: 6px;
  border-radius: 3px;
  background: #64d2ff;
}
.vram-label {
  font-size: 12px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.50);
  min-width: 85px;
}
progressbar.vram-bar { min-height: 5px; }
progressbar.vram-bar trough {
  min-height: 5px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.08);
  border: none;
  box-shadow: none;
}
progressbar.vram-bar progress {
  min-height: 5px;
  border-radius: 3px;
  background: #00d2d3;
}
.ram-label {
  font-size: 12px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.50);
  min-width: 85px;
}
progressbar.ram-bar { min-height: 5px; }
progressbar.ram-bar trough {
  min-height: 5px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.08);
  border: none;
  box-shadow: none;
}
progressbar.ram-bar progress {
  min-height: 5px;
  border-radius: 3px;
  background: #a855f7;
}
.disk-label {
  font-size: 12px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.50);
  min-width: 85px;
}
progressbar.disk-bar { min-height: 5px; }
progressbar.disk-bar trough {
  min-height: 5px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.08);
  border: none;
  box-shadow: none;
}
progressbar.disk-bar progress {
  min-height: 5px;
  border-radius: 3px;
  background: #4ade80;
}
.proc-name {
  font-size: 10px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.60);
  min-width: 85px;
}
.proc-cpu {
  font-size: 10px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.45);
  min-width: 42px;
}
.proc-mem {
  font-size: 10px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.45);
  min-width: 42px;
}
.net-label {
  font-size: 12px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.55);
  padding: 2px 0;
}
.stat {
  font-size: 11px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.45);
}
"#;
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(css);
    gtk4::style_context_add_provider_for_display(
        &gtk4::prelude::WidgetExt::display(&window),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );

    window.present();

    // --- Timer ---
    gtk4::glib::timeout_add_seconds_local(POLL_INTERVAL, move || {
        let mut st = state.borrow_mut();

        if let Some(snapshots) = monitor::read_all_cpu_snapshots() {
            if st.prev_snapshots.len() >= 2 && snapshots.len() == st.prev_snapshots.len() {
                let agg = monitor::compute_cpu_usage(&st.prev_snapshots[0], &snapshots[0])
                    .clamp(0.0, 100.0);

                st.history.push(agg);
                if agg > st.max_avg {
                    st.max_avg = agg;
                }
                st.sum_avg += agg;
                st.sample_count += 1;

                usage_label.set_text(&format!("{:.0}%", agg));
                progress_bar.set_fraction(agg / 100.0);
                max_label.set_text(&format!("Max {:.0}%", st.max_avg));
                avg_label.set_text(&format!(
                    "Avg {:.0}%",
                    st.sum_avg / st.sample_count as f64
                ));
                sparkline.queue_draw();

                for (i, (bar, pct)) in core_widgets.iter().enumerate() {
                    let idx = i + 1;
                    if idx < snapshots.len() && idx < st.prev_snapshots.len() {
                        let usage = monitor::compute_cpu_usage(
                            &st.prev_snapshots[idx],
                            &snapshots[idx],
                        )
                        .clamp(0.0, 100.0);
                        bar.set_fraction(usage / 100.0);
                        pct.set_text(&format!("{:.0}%", usage));
                    }
                }
            }
            st.prev_snapshots = snapshots;
        }

        if let Some(ref s) = sensor {
            if let Some(temp) = monitor::read_temp(s) {
                temp_label.set_text(&format!("{:.0}°", temp));
            }
        }

        if let Some(gpu) = monitor::read_gpu_stats() {
            st.gpu_history.push(gpu.usage);
            gpu_bar.set_fraction(gpu.usage / 100.0);
            gpu_usage_label.set_text(&format!("{:.0}%", gpu.usage));
            gpu_temp_label.set_text(&format!("{:.0}°", gpu.temp));
            vram_bar.set_fraction(gpu.mem_percent() / 100.0);
            vram_label.set_text(&format!(
                "{:.1} / {:.0} GB",
                gpu.mem_used_mib / 1024.0,
                gpu.mem_total_mib / 1024.0
            ));
            gpu_sparkline.queue_draw();
        } else {
            gpu_bar.set_fraction(0.0);
            gpu_usage_label.set_text("--%");
            gpu_temp_label.set_text("--°");
            vram_bar.set_fraction(0.0);
            vram_label.set_text("-- / -- GB");
        }

        if let Some(ram) = monitor::read_ram_stats() {
            let pct = ram.used_percent();
            st.ram_history.push(pct);
            ram_bar.set_fraction(pct / 100.0);
            ram_label.set_text(&format!(
                "{:.1} / {:.0} GB",
                ram.used_gb(),
                ram.total_gb()
            ));
            ram_sparkline.queue_draw();
        } else {
            ram_bar.set_fraction(0.0);
            ram_label.set_text("-- / -- GB");
        }

        if let Some(disk) = monitor::read_disk_stats("/") {
            disk_bar.set_fraction(disk.used_percent() / 100.0);
            disk_label.set_text(&format!(
                "{:.1} / {:.0} GB",
                disk.used_gb(),
                disk.total_gb()
            ));
        } else {
            disk_bar.set_fraction(0.0);
            disk_label.set_text("-- / -- GB");
        }

        let procs = monitor::read_top_processes(4);
        for (i, row) in proc_rows.iter().enumerate() {
            if let Some(p) = procs.get(i) {
                let name = if p.name.len() > 12 {
                    format!("{}…", &p.name[..11])
                } else {
                    p.name.clone()
                };
                row.name.set_text(&name);
                row.cpu.set_text(&format!("{:.1}%", p.cpu_percent));
                row.mem.set_text(&format!("{:.1}%", p.mem_percent));
            } else {
                row.name.set_text("");
                row.cpu.set_text("");
                row.mem.set_text("");
            }
        }

        if let Some(net) = net_monitor.borrow_mut().poll() {
            let rx = net.rx_bytes_per_sec;
            let tx = net.tx_bytes_per_sec;
            let rx_s = if rx >= 1024.0 * 1024.0 {
                format!("{:.1} MB/s", rx / 1024.0 / 1024.0)
            } else if rx >= 1024.0 {
                format!("{:.0} KB/s", rx / 1024.0)
            } else {
                format!("{:.0} B/s", rx)
            };
            let tx_s = if tx >= 1024.0 * 1024.0 {
                format!("{:.1} MB/s", tx / 1024.0 / 1024.0)
            } else if tx >= 1024.0 {
                format!("{:.0} KB/s", tx / 1024.0)
            } else {
                format!("{:.0} B/s", tx)
            };
            net_label.set_text(&format!("▼ {}  ▲ {}", rx_s, tx_s));
        }

        gtk4::glib::ControlFlow::Continue
    });
}
