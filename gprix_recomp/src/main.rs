mod audio;
mod bus;
mod cpu;
mod dos;
mod loader;
mod recompiler;
pub mod sid_player;

use std::path::Path;

use bus::MemoryBus;
use cpu::CpuContext;
use dos::DosEnvironment;
use loader::load_mz_executable;
use recompiler::Recompiler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("====================================================");
    println!("  Grand Prix Circuit (MS-DOS) Static Recompilation  ");
    println!("              Native Portable Rust Core             ");
    println!("====================================================");

    let mut bus = MemoryBus::new();
    let mut cpu = CpuContext::new();

    // Ensure working directory is the folder containing the executable
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let _ = std::env::set_current_dir(exe_dir);
        }
    }

    // Check path to MS-DOS executable (supports original retail GPEGA.EXE or unpacked GPEGA_UNP.EXE)
    let candidate_exes = [
        "GPEGA.EXE",
        "GPEGA_UNP.EXE",
        "gprix2/GPEGA.EXE",
        "gprix2/GPEGA_UNP.EXE",
        "assets/GPEGA.EXE",
        "assets/GPEGA_UNP.EXE",
        "../gprix2/GPEGA.EXE",
        "../gprix2/GPEGA_UNP.EXE",
        "../GPEGA.EXE",
        "../GPEGA_UNP.EXE",
    ];

    let found_exe = candidate_exes.iter().find(|p| Path::new(p).exists());

    let exe_path = match found_exe {
        Some(p) => *p,
        None => {
            eprintln!("\n========================================================================");
            eprintln!("              Grand Prix Circuit (1988) - Native Recompilation");
            eprintln!("========================================================================");
            eprintln!(" [ERROR] Original MS-DOS game files not found!");
            eprintln!("");
            eprintln!(" To play, please copy the original game files from your legally owned");
            eprintln!(" copy of Grand Prix Circuit into this folder (or inside a 'gprix2/' folder):");
            eprintln!("");
            eprintln!("   Required:");
            eprintln!("     * GPEGA.EXE (or GPEGA_UNP.EXE)");
            eprintln!("     * Track & car files (*.PCS, *.PES, e.g. STATUS.PES, CAR1.PES, etc.)");
            eprintln!("");
            eprintln!("   Optional (for Commodore 64 Mode audio):");
            eprintln!("     * Grand_Prix_Circuit.sid");
            eprintln!("========================================================================\n");
            return Ok(());
        }
    };

    let asset_dir = if Path::new("gprix2").exists() {
        "gprix2"
    } else if Path::new("assets").exists() {
        "assets"
    } else if Path::new("../gprix2").exists() {
        "../gprix2"
    } else {
        "."
    };

    println!("[INIT] Loading MZ binary: {}", exe_path);
    println!("[INIT] Asset directory: {}", asset_dir);

    let mut dos = DosEnvironment::new(asset_dir);

    // Load executable at base segment 0x1000 (physical 0x10000)
    let psp_seg = 0x1000;
    let prog = load_mz_executable(exe_path, &mut bus, &mut cpu, psp_seg)?;

    println!("[LOADER] Program loaded successfully:");
    println!("  PSP Segment:   0x{:04X}", prog.psp_segment);
    println!("  Load Segment:  0x{:04X}", prog.load_segment);
    println!("  Entry Point:   {:04X}:{:04X}", cpu.cs, cpu.ip);
    println!("  Initial Stack: {:04X}:{:04X}", cpu.ss, cpu.sp);

    // Disassemble initial entry point
    let recomp = Recompiler::new(cpu.cs);
    recomp.dump_instructions(&mut bus, cpu.ip, 15);

    // Run steps to trace startup initialization
    println!("\n[EXEC] Executing game initialization...");
    let mut steps: u64 = 0;
    while steps < 20_000 && !cpu.halted {
        steps += 1;
        if !recomp.step(&mut cpu, &mut bus, &mut dos) {
            break;
        }
    }
    println!("[EXEC] Initialization reached step {}", steps);

    // Create 320x200 window scaled 3x (960x600)
    println!("\n[WINDOW] Opening native EGA display window (320x200 scaled 3x)...");
    let mut window = match minifb::Window::new(
        "Grand Prix Circuit (1988) - Native Recompilation (Rust)",
        320,
        200,
        minifb::WindowOptions {
            resize: true,
            scale: minifb::Scale::X4,
            ..minifb::WindowOptions::default()
        },
    ) {
        Ok(win) => win,
        Err(e) => {
            eprintln!("[WINDOW ERROR] Could not create window: {}", e);
            return Ok(());
        }
    };

    window.set_target_fps(60);
    let mut framebuffer = vec![0u32; 320 * 200];

    println!("[EXEC] Entering interactive native loop. Close window to exit.");
    let mut frame_count: u64 = 0;
    let mut timer_accum: f64 = 0.0;

    // 10 slices of 35,000 instructions = 350,000 instructions per 60Hz frame (~21 MHz CPU equivalent)
    const SLICES_PER_FRAME: usize = 10;
    const INSTRUCTIONS_PER_SLICE: usize = 35_000;

    while window.is_open() {
        let pit_freq = bus.pit_ch0_frequency();
        let tick_delta = (pit_freq / 60.0) / (SLICES_PER_FRAME as f64);

        for _ in 0..SLICES_PER_FRAME {
            if cpu.halted {
                break;
            }

            timer_accum += tick_delta;
            if timer_accum > 10.0 {
                timer_accum = 10.0;
            }

            for _ in 0..INSTRUCTIONS_PER_SLICE {
                if cpu.halted {
                    break;
                }

                // Check pending hardware timer interrupt (PIT IRQ0 -> INT 08h)
                if timer_accum >= 1.0 && cpu.flags.if_ {
                    if bus.trigger_interrupt(&mut cpu, 8) {
                        timer_accum -= 1.0;
                        dos.timer_ticks = dos.timer_ticks.wrapping_add(1);
                        let bda_addr = 0x46C;
                        bus.ram[bda_addr] = (dos.timer_ticks & 0xFF) as u8;
                        bus.ram[bda_addr + 1] = ((dos.timer_ticks >> 8) & 0xFF) as u8;
                        bus.ram[bda_addr + 2] = ((dos.timer_ticks >> 16) & 0xFF) as u8;
                        bus.ram[bda_addr + 3] = ((dos.timer_ticks >> 24) & 0xFF) as u8;
                    } else {
                        // Standard BIOS tick when no custom vector is hooked (seg < 0x1010)
                        timer_accum -= 1.0;
                        dos.timer_ticks = dos.timer_ticks.wrapping_add(1);
                        let bda_addr = 0x46C;
                        bus.ram[bda_addr] = (dos.timer_ticks & 0xFF) as u8;
                        bus.ram[bda_addr + 1] = ((dos.timer_ticks >> 8) & 0xFF) as u8;
                        bus.ram[bda_addr + 2] = ((dos.timer_ticks >> 16) & 0xFF) as u8;
                        bus.ram[bda_addr + 3] = ((dos.timer_ticks >> 24) & 0xFF) as u8;
                        bus.trigger_interrupt(&mut cpu, 0x1C);
                    }
                }

                if !recomp.step(&mut cpu, &mut bus, &mut dos) {
                    break;
                }
            }
        }

        // Optional diagnostics via GPRIX_DEBUG
        if std::env::var("GPRIX_DEBUG").is_ok() && frame_count % 60 == 0 && frame_count >= 300 {
            let ivt8 = bus.get_ivt_vector(8);
            let t_low = bus.read_u16(0x209F, 0x7FE8);
            let t_high = bus.read_u16(0x209F, 0x7FEA);
            let reload = bus.read_u16(0x209F, 0x7FF0);
            let countdown = bus.read_u16(0x209F, 0x7FF2);
            println!(
                "[TIMER DIAG] frame: {} | CS:IP: {:04X}:{:04X} | DS: {:04X} | IF: {} | IVT8: {:04X}:{:04X} | PIT ch0: {} ({:.1}Hz) | ticks_209f: {:04X}:{:04X} | cd: {}/{} | timer_accum: {:.2}",
                frame_count, cpu.cs, cpu.ip, cpu.ds, cpu.flags.if_, ivt8.0, ivt8.1,
                bus.pit_ch0_divisor, bus.pit_ch0_frequency(),
                t_high, t_low, countdown, reload, timer_accum
            );
        }

        // Process keyboard inputs from window (Press events -> IRQ1 / INT 09h)
        for key in window.get_keys_pressed(minifb::KeyRepeat::No) {
            if key == minifb::Key::F9 {
                let is_smooth = bus.toggle_smooth_mode();
                if is_smooth {
                    println!("\n>>> [FPS SMOOTHING] Activated 60 FPS Spatial Motion Interpolation (Zero Blur Scanline Warp)! <<<");
                } else {
                    println!("\n>>> [FPS SMOOTHING] Activated Classic 20 FPS Mode! <<<");
                }
                continue;
            }

            if key == minifb::Key::F10 {
                let new_mode = bus.toggle_mode();
                match new_mode {
                    bus::VideoAudioMode::C64 => {
                        println!("\n>>> [MODE SWITCH] Activated COMMODORE 64 (C64) Mode! (VIC-II Palette + SID 6581 Audio) <<<");
                    }
                    bus::VideoAudioMode::Dos => {
                        println!("\n>>> [MODE SWITCH] Activated MS-DOS Mode! (EGA Palette + PC Speaker / AdLib Audio) <<<");
                    }
                }
                continue;
            }

            let (ascii, scancode) = map_key(key);
            if scancode != 0 {
                dos.key_queue.push_back((ascii, scancode));
                bus.kbd_port_60 = scancode;
                bus.trigger_interrupt(&mut cpu, 9);
            }
        }

        // Process key releases (Break code scancode | 0x80 -> IRQ1 / INT 09h)
        for key in window.get_keys_released() {
            let (_, scancode) = map_key(key);
            if scancode != 0 {
                bus.kbd_port_60 = scancode | 0x80;
                bus.trigger_interrupt(&mut cpu, 9);
            }
        }

        // Render EGA planar memory to RGB framebuffer
        bus.render_ega_to_rgb(&mut framebuffer);

        // Present frame
        if let Err(e) = window.update_with_buffer(&framebuffer, 320, 200) {
            eprintln!("[WINDOW ERROR] Buffer update error: {}", e);
            break;
        }

        frame_count += 1;
        if frame_count % 300 == 0 {
            println!("[FRAME] Rendered frame {}, current CS:IP = {:04X}:{:04X}", frame_count, cpu.cs, cpu.ip);
        }
    }

    println!("\n[OK] Core execution finished smoothly.");
    Ok(())
}

fn map_key(key: minifb::Key) -> (u8, u8) {
    match key {
        minifb::Key::Enter => (0x0D, 0x1C),
        minifb::Key::Space => (b' ', 0x39),
        minifb::Key::Escape => (0x1B, 0x01),
        minifb::Key::Backspace => (0x08, 0x0E),
        minifb::Key::Tab => (0x09, 0x0F),
        minifb::Key::Up | minifb::Key::NumPad8 => (0x00, 0x48),
        minifb::Key::Down | minifb::Key::NumPad2 => (0x00, 0x50),
        minifb::Key::Left | minifb::Key::NumPad4 => (0x00, 0x4B),
        minifb::Key::Right | minifb::Key::NumPad6 => (0x00, 0x4D),
        minifb::Key::NumPad5 => (0x00, 0x4C),
        minifb::Key::LeftShift => (0x00, 0x2A),
        minifb::Key::RightShift => (0x00, 0x36),
        minifb::Key::LeftCtrl => (0x00, 0x1D),
        minifb::Key::RightCtrl => (0x00, 0x1D),
        minifb::Key::LeftAlt => (0x00, 0x38),
        minifb::Key::RightAlt => (0x00, 0x38),
        minifb::Key::Key0 => (b'0', 0x0B),
        minifb::Key::Key1 => (b'1', 0x02),
        minifb::Key::Key2 => (b'2', 0x03),
        minifb::Key::Key3 => (b'3', 0x04),
        minifb::Key::Key4 => (b'4', 0x05),
        minifb::Key::Key5 => (b'5', 0x06),
        minifb::Key::Key6 => (b'6', 0x07),
        minifb::Key::Key7 => (b'7', 0x08),
        minifb::Key::Key8 => (b'8', 0x09),
        minifb::Key::Key9 => (b'9', 0x0A),
        minifb::Key::A => (b'a', 0x1E),
        minifb::Key::B => (b'b', 0x30),
        minifb::Key::C => (b'c', 0x2E),
        minifb::Key::D => (b'd', 0x20),
        minifb::Key::E => (b'e', 0x12),
        minifb::Key::F => (b'f', 0x21),
        minifb::Key::G => (b'g', 0x22),
        minifb::Key::H => (b'h', 0x23),
        minifb::Key::I => (b'i', 0x17),
        minifb::Key::J => (b'j', 0x24),
        minifb::Key::K => (b'k', 0x25),
        minifb::Key::L => (b'l', 0x26),
        minifb::Key::M => (b'm', 0x32),
        minifb::Key::N => (b'n', 0x31),
        minifb::Key::O => (b'o', 0x18),
        minifb::Key::P => (b'p', 0x19),
        minifb::Key::Q => (b'q', 0x10),
        minifb::Key::R => (b'r', 0x13),
        minifb::Key::S => (b's', 0x1F),
        minifb::Key::T => (b't', 0x14),
        minifb::Key::U => (b'u', 0x16),
        minifb::Key::V => (b'v', 0x2F),
        minifb::Key::W => (b'w', 0x11),
        minifb::Key::X => (b'x', 0x2D),
        minifb::Key::Y => (b'y', 0x15),
        minifb::Key::Z => (b'z', 0x2C),
        _ => (0, 0),
    }
}
