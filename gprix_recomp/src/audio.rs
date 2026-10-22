/// Dual Audio System: Real-time PC Speaker (DOS) + MOS 6581 SID (Commodore 64) synthesis using cpal.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crate::sid_player::SidPlayer;

pub struct PcSpeaker {
    pub enabled: Arc<AtomicBool>,
    pub freq_hz: Arc<AtomicU32>,
    pub is_c64: Arc<AtomicBool>,
    pub sid: Arc<Mutex<Option<SidPlayer>>>,
    _stream: Option<cpal::Stream>,
}

impl PcSpeaker {
    pub fn new() -> Self {
        let enabled = Arc::new(AtomicBool::new(false));
        let freq_hz = Arc::new(AtomicU32::new(440));
        let is_c64 = Arc::new(AtomicBool::new(false));
        let sid = Arc::new(Mutex::new(None));

        let is_c64_clone = is_c64.clone();
        let sid_clone = sid.clone();

        let stream = (|| -> Option<cpal::Stream> {
            let host = cpal::default_host();
            let device = match host.default_output_device() {
                Some(d) => d,
                None => {
                    eprintln!("[AUDIO] No default audio output device found.");
                    return None;
                }
            };

            let config = match device.default_output_config() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[AUDIO] Failed to get default output config: {}", e);
                    return None;
                }
            };

            let sample_rate_u32 = config.sample_rate() as u32;
            println!(
                "[AUDIO] Initialized device, format {:?}, sample_rate {} Hz, channels {}",
                config.sample_format(),
                sample_rate_u32,
                config.channels()
            );

            // Initialize SID Player at native output sample rate
            if let Some(player) = SidPlayer::new(sample_rate_u32) {
                if let Ok(mut lock) = sid_clone.lock() {
                    *lock = Some(player);
                }
                println!("[AUDIO] Commodore 64 SID 6581 (Kris Hatlelid) engine initialized.");
            }

            let sample_rate = sample_rate_u32 as f32;
            let channels = config.channels() as usize;
            let stream_config: cpal::StreamConfig = config.clone().into();
            let err_fn = |err| eprintln!("[AUDIO ERROR] Stream error: {}", err);

            let en1 = enabled.clone();
            let fr1 = freq_hz.clone();
            let is_c64_cb = is_c64_clone.clone();
            let sid_cb = sid_clone.clone();
            let mut phase = 0.0f32;

            let stream_res = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_output_stream(
                    stream_config,
                    move |data: &mut [f32], _| {
                        let c64_active = is_c64_cb.load(Ordering::Relaxed);
                        let on = en1.load(Ordering::Relaxed);
                        let freq = fr1.load(Ordering::Relaxed) as f32;

                        for frame in data.chunks_mut(channels) {
                            let sample = if c64_active {
                                if let Ok(mut lock) = sid_cb.try_lock() {
                                    if let Some(ref mut p) = *lock {
                                        p.render_sample()
                                    } else {
                                        0.0
                                    }
                                } else {
                                    0.0
                                }
                            } else if on && freq >= 20.0 && freq <= 15000.0 {
                                let s = if phase < 0.5 { 0.15 } else { -0.15 };
                                phase += freq / sample_rate;
                                if phase >= 1.0 {
                                    phase -= 1.0;
                                }
                                s
                            } else {
                                0.0
                            };
                            for ch in frame.iter_mut() {
                                *ch = sample;
                            }
                        }
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_output_stream(
                    stream_config,
                    move |data: &mut [i16], _| {
                        let c64_active = is_c64_cb.load(Ordering::Relaxed);
                        let on = en1.load(Ordering::Relaxed);
                        let freq = fr1.load(Ordering::Relaxed) as f32;

                        for frame in data.chunks_mut(channels) {
                            let sample_f32 = if c64_active {
                                if let Ok(mut lock) = sid_cb.try_lock() {
                                    if let Some(ref mut p) = *lock {
                                        p.render_sample()
                                    } else {
                                        0.0
                                    }
                                } else {
                                    0.0
                                }
                            } else if on && freq >= 20.0 && freq <= 15000.0 {
                                let s = if phase < 0.5 { 0.15 } else { -0.15 };
                                phase += freq / sample_rate;
                                if phase >= 1.0 {
                                    phase -= 1.0;
                                }
                                s
                            } else {
                                0.0
                            };
                            let sample = (sample_f32 * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            for ch in frame.iter_mut() {
                                *ch = sample;
                            }
                        }
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_output_stream(
                    stream_config,
                    move |data: &mut [u16], _| {
                        let c64_active = is_c64_cb.load(Ordering::Relaxed);
                        let on = en1.load(Ordering::Relaxed);
                        let freq = fr1.load(Ordering::Relaxed) as f32;

                        for frame in data.chunks_mut(channels) {
                            let sample_f32 = if c64_active {
                                if let Ok(mut lock) = sid_cb.try_lock() {
                                    if let Some(ref mut p) = *lock {
                                        p.render_sample()
                                    } else {
                                        0.0
                                    }
                                } else {
                                    0.0
                                }
                            } else if on && freq >= 20.0 && freq <= 15000.0 {
                                let s = if phase < 0.5 { 0.15 } else { -0.15 };
                                phase += freq / sample_rate;
                                if phase >= 1.0 {
                                    phase -= 1.0;
                                }
                                s
                            } else {
                                0.0
                            };
                            let sample = (32768.0 + sample_f32 * 32767.0).clamp(0.0, 65535.0) as u16;
                            for ch in frame.iter_mut() {
                                *ch = sample;
                            }
                        }
                    },
                    err_fn,
                    None,
                ),
                format => {
                    eprintln!("[AUDIO] Unsupported sample format: {:?}", format);
                    return None;
                }
            };

            match stream_res {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        eprintln!("[AUDIO] Failed to start stream play: {}", e);
                    }
                    Some(stream)
                }
                Err(e) => {
                    eprintln!("[AUDIO] Failed to build output stream: {}", e);
                    None
                }
            }
        })();

        Self {
            enabled,
            freq_hz,
            is_c64,
            sid,
            _stream: stream,
        }
    }

    pub fn set_state(&self, on: bool, freq: u32) {
        self.enabled.store(on, Ordering::Relaxed);
        self.freq_hz.store(freq, Ordering::Relaxed);
    }

    pub fn set_c64_mode(&self, enabled: bool) {
        self.is_c64.store(enabled, Ordering::Relaxed);
    }

    pub fn set_c64_song(&self, song0: u8) {
        if let Ok(mut lock) = self.sid.lock() {
            if let Some(ref mut p) = *lock {
                p.set_song(song0);
            }
        }
    }
}
