use super::*;

impl WgpuAppShell {
    pub(super) fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_artwork();
        if let Some(live_session) = self.live_session.as_mut() {
            if let Err(error) = live_session.update() {
                self.error = Some(error.to_string());
                event_loop.exit();
                return;
            }
            if live_session.is_finished() {
                if let Err(error) = self.finish_live_session() {
                    self.error = Some(error.to_string());
                    event_loop.exit();
                    return;
                }
                self.state.screen = AppScreen::Results;
                self.result_row_index = 0;
            }
        }

        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let width = gpu.config.width;
        let height = gpu.config.height;
        if let Some(artwork) = &self.artwork_renderer {
            artwork.prepare(
                &gpu.queue,
                width,
                height,
                if self.state.screen == AppScreen::Gameplay {
                    0.10
                } else {
                    0.16
                },
            );
        }
        gpu.window.set_title(&self.window_title());

        let now = Instant::now();
        let mut frame_interval_ms = None;
        if let Some(previous) = self.last_redraw.replace(now) {
            let milliseconds = previous.elapsed().as_secs_f64() * 1_000.0;
            self.frame_interval_ms.push(milliseconds);
            frame_interval_ms = Some(milliseconds);
        }

        let payload = self.render_payload(width, height);
        let sample = match payload {
            WgpuAppRenderPayload::Rects(rects) => {
                let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.ui_renderer.as_mut())
                else {
                    return;
                };
                renderer
                    .render_with_artwork(
                        gpu,
                        WgpuRectFrame {
                            rects: &rects,
                            clear_color: CLEAR_COLOR,
                        },
                        self.artwork_renderer.as_ref(),
                    )
                    .map(|sample| WgpuAppFrameSample {
                        acquire_surface_ms: sample.acquire_surface_ms,
                        encode_submit_present_ms: sample.encode_submit_present_ms,
                        vertex_count: sample.vertex_count,
                    })
            }
            WgpuAppRenderPayload::Highway {
                sprites,
                active_lanes,
                lane_count,
                song_time_seconds,
                end_seconds,
                overlay,
            } => {
                let (Some(gpu), Some(renderer)) =
                    (self.gpu.as_mut(), self.highway_renderer.as_mut())
                else {
                    return;
                };
                renderer
                    .render_with_overlay_and_artwork(
                        gpu,
                        WgpuHighwayFrame {
                            sprites: &sprites,
                            lane_count,
                            active_lanes: &active_lanes,
                            song_time_seconds,
                            end_seconds,
                        },
                        &overlay,
                        self.artwork_renderer.as_ref(),
                    )
                    .map(|sample| WgpuAppFrameSample {
                        acquire_surface_ms: sample.acquire_surface_ms,
                        encode_submit_present_ms: sample.encode_submit_present_ms,
                        vertex_count: sample.vertex_count,
                    })
            }
        };

        match sample {
            Ok(sample) => {
                self.acquire_surface_ms.push(sample.acquire_surface_ms);
                self.encode_submit_present_ms
                    .push(sample.encode_submit_present_ms);
                self.vertices_per_frame.push(sample.vertex_count as f64);
                self.rendered_frames += 1;
                if let Some(live_session) = self.live_session.as_mut() {
                    live_session.record_render_sample(
                        frame_interval_ms.unwrap_or_default(),
                        sample.acquire_surface_ms + sample.encode_submit_present_ms,
                    );
                }
            }
            Err(WgpuFrameError::Recoverable(reason)) => {
                println!("app_wgpu_frame_skipped={reason}");
            }
            Err(WgpuFrameError::Fatal(reason)) => {
                self.error = Some(reason);
                event_loop.exit();
                return;
            }
        }

        if self.should_exit_after_frame() {
            event_loop.exit();
        } else if self.should_continue_redrawing()
            && self
                .state
                .settings
                .video
                .target_frame_rate
                .filter(|rate| *rate > 0)
                .is_none()
        {
            self.request_redraw();
        }
    }

    fn render_payload(&self, width: u32, height: u32) -> WgpuAppRenderPayload {
        if self.state.screen == AppScreen::Gameplay
            && let Some(live_session) = self.live_session.as_ref()
        {
            let snapshot = live_session.snapshot();
            let layout = wgpu_highway_render_layout(
                width,
                height,
                snapshot.chart.lane_count() as usize,
                self.state.settings.gameplay.scroll_time_seconds(),
            );
            let sprites = build_highway_note_sprites(
                layout,
                snapshot.chart,
                snapshot.judged_note_ids,
                snapshot.song_time_seconds,
            );
            return WgpuAppRenderPayload::Highway {
                sprites,
                active_lanes: snapshot.active_lanes.to_vec(),
                lane_count: snapshot.chart.lane_count() as usize,
                song_time_seconds: snapshot.song_time_seconds,
                end_seconds: snapshot.end_seconds,
                overlay: self.gameplay_overlay(width as f32, height as f32),
            };
        }

        WgpuAppRenderPayload::Rects(self.build_ui_rects(width as f32, height as f32))
    }
}

impl WgpuAppShell {
    pub(super) fn window_title(&self) -> String {
        format!(
            "Zeff Rhythm | {} | {}",
            screen_header(self.state.screen),
            selected_label(self)
        )
    }

    pub(super) fn request_redraw(&self) {
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }

    pub(super) fn should_continue_redrawing(&self) -> bool {
        self.options.max_seconds.is_some() || self.state.screen == AppScreen::Gameplay
    }

    pub(super) fn should_exit_after_frame(&self) -> bool {
        self.options
            .max_seconds
            .zip(self.start)
            .is_some_and(|(seconds, start)| start.elapsed().as_secs_f64() >= seconds)
    }

    pub(super) fn print_summary(&self) {
        if let Some(gpu) = self.gpu.as_ref() {
            println!(
                "app_wgpu_adapter name=\"{}\" backend={:?} device_type={:?}",
                gpu.adapter_info.name, gpu.adapter_info.backend, gpu.adapter_info.device_type
            );
            println!(
                "app_wgpu_surface size={}x{} format={:?}",
                gpu.config.width, gpu.config.height, gpu.config.format
            );
            println!(
                "app_wgpu_present requested={} selected={:?} supported={}",
                self.state
                    .settings
                    .video
                    .render_latency
                    .present_mode
                    .as_str(),
                gpu.config.present_mode,
                format_present_modes(&gpu.supported_present_modes)
            );
        }

        println!(
            "app_wgpu frames={} screen={}",
            self.rendered_frames,
            screen_label(self.state.screen)
        );
        crate::play::metrics::print_metric("app_wgpu_frame_interval_ms", &self.frame_interval_ms);
        crate::play::metrics::print_metric("app_wgpu_acquire_surface_ms", &self.acquire_surface_ms);
        crate::play::metrics::print_metric(
            "app_wgpu_encode_submit_present_ms",
            &self.encode_submit_present_ms,
        );
        crate::play::metrics::print_metric("app_wgpu_vertices", &self.vertices_per_frame);
    }
}

impl ApplicationHandler for WgpuAppShell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Zeff Rhythm")
            .with_min_inner_size(PhysicalSize::new(760, 520))
            .with_inner_size(PhysicalSize::new(
                self.options.width.max(760),
                self.options.height.max(520),
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.error = Some(format!("failed to create winit window: {error}"));
                event_loop.exit();
                return;
            }
        };
        let gpu = match block_on(WgpuSurfaceState::new(
            event_loop,
            window,
            self.state.settings.video.render_latency,
            self.options.power_preference,
        )) {
            Ok(gpu) => gpu,
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        };
        let ui_renderer = WgpuRectRenderer::new(&gpu.device, gpu.config.format);
        let highway_renderer = WgpuHighwayRenderer::new(&gpu.device, gpu.config.format);
        self.artwork_renderer = Some(crate::render::wgpu_artwork::WgpuArtworkRenderer::new(
            &gpu.device,
            gpu.config.format,
        ));

        println!(
            "app_wgpu=started screen={} present={} frame_latency={}",
            screen_label(self.state.screen),
            self.state
                .settings
                .video
                .render_latency
                .present_mode
                .as_str(),
            self.state
                .settings
                .video
                .render_latency
                .desired_maximum_frame_latency
        );
        gpu.window.request_redraw();
        if self.state.settings.video.fullscreen {
            gpu.window
                .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        self.start = Some(Instant::now());
        self.ui_renderer = Some(ui_renderer);
        self.highway_renderer = Some(highway_renderer);
        self.gpu = Some(gpu);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Focused(focused) => {
                if !focused {
                    self.stop_song_preview();
                    self.modifiers = ModifiersState::empty();
                    self.volume_scroll_remainder = 0.0;
                    self.clear_preedit();
                }
                self.window_focused = focused;
                if let Err(error) = self.process_winit_focus(focused) {
                    self.error = Some(error.to_string());
                    event_loop.exit();
                }
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if let Err(error) = self.click_at(event_loop) {
                    self.library_launch_error = Some(error.to_string());
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.handle_volume_wheel(delta) {
                    return;
                }
                if self.state.screen != AppScreen::SongSelect
                    || self.asset_load.is_some()
                    || self.help_visible
                    || self.binding_capture.is_some()
                {
                    return;
                }
                let amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                if amount != 0.0 {
                    self.move_library_selection(if amount > 0.0 { -1 } else { 1 });
                    self.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => self.add_library_root(path),
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Ime(ime) => self.process_search_ime(ime),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    if event.state == ElementState::Pressed {
                        if let Some(text) = event.text.as_deref() {
                            self.insert_search_text(text);
                        }
                    }
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if self.search_input(code, event.text.as_deref(), pressed) {
                    return;
                }
                if let Err(error) = self.handle_key(event_loop, code, pressed, event.repeat) {
                    self.error = Some(error.to_string());
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => self.render_frame(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.update_search_ime();
        self.refresh_library();
        self.poll_chart_load();
        self.poll_artwork();
        self.poll_song_preview();
        if self.should_continue_redrawing() {
            if let Some(rate) = self
                .state
                .settings
                .video
                .target_frame_rate
                .filter(|rate| *rate > 0)
            {
                let next = self.last_redraw.unwrap_or_else(Instant::now)
                    + std::time::Duration::from_secs_f64(1.0 / f64::from(rate));
                event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                if Instant::now() >= next {
                    self.request_redraw();
                }
            } else {
                event_loop.set_control_flow(ControlFlow::Poll);
            }
        } else if self.library_scanner.is_scanning()
            || self.asset_load.is_some()
            || self.artwork_loader.is_loading()
            || self.preview_loader.is_loading()
        {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + std::time::Duration::from_millis(100),
            ));
            self.request_redraw();
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

pub(super) struct WgpuAppFrameSample {
    acquire_surface_ms: f64,
    encode_submit_present_ms: f64,
    vertex_count: usize,
}

enum WgpuAppRenderPayload {
    Rects(Vec<WgpuRect>),
    Highway {
        sprites: Vec<HighwayNoteSprite>,
        active_lanes: Vec<bool>,
        lane_count: usize,
        song_time_seconds: f64,
        end_seconds: f64,
        overlay: Vec<WgpuRect>,
    },
}
