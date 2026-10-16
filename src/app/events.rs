use super::*;
impl App {
    pub(super) fn handle_events(&mut self, ctx: &egui::Context) {
        if let Some(p) = &mut self.pending
            && p.from_picker
            && !ctx.input(|i| i.viewport().focused.unwrap_or(false))
        {
            p.flow.cancel_auto();
        }
        while let Ok(library) = self.activation.try_recv() {
            if library {
                self.open_library()
            } else {
                self.show_picker(ctx)
            }
        }
        if self.shortcut.as_ref().is_some_and(Shortcut::triggered) {
            self.show_picker(ctx);
        }
        while let Ok(e) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            match e.id.as_ref() {
                "show" => self.show_picker(ctx),
                "library" => self.open_library(),
                "hide" => {
                    self.hide_picker(ctx);
                    self.library_open = false;
                }
                "quit" => self.quit = true,
                _ => {}
            }
        }
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::UpdateHealthFailed(error) => { self.message(error, true); self.quit = true; },
                Event::Show => self.show_picker(ctx),
                Event::Progress(s) => self.message(s, false),
                Event::Catalog(id, result) => {
                    if id == self.catalog_id {
                        self.browsing = false;
                        match result {
                            Ok(p) => {
                                self.catalog = p;
                                self.message(if self.catalog.is_empty() { "No packs found" } else { "Select a pack to preview" }, false);
                            }
                            Err(e) => self.message(format!("{e:#}"), true),
                        }
                    }
                }
                Event::Preview(id, result) => {
                    if id != self.catalog_id { continue; }
                    self.browsing = false;
                    match result {
                        Ok(p) => self.preview = Some(p),
                        Err(e) => self.message(format!("{e:#}"), true),
                    }
                }
                Event::Installed(result) => {
                    self.installing = false;
                    match result {
                        // The favorite was committed before downloading. Do not
                        // overwrite an unfavorite/removal made during the download.
                        Ok(p) => { self.message(format!("Downloaded {} · available offline", p.title), false); self.refresh(); }
                        Err(e) => self.message(format!("Download incomplete: {e:#}. Open the pack and retry; cached images are reused."), true),
                    }
                }
                Event::Media(url, result, _permit) => {
                    self.inflight.remove(&url);
                    match result {
                        Ok((_asset, decoded)) => {
                            let mut frames = Vec::new();
                            let mut delays = Vec::new();
                            let mut bytes = 0;
                            for (i, f) in decoded.frames.into_iter().enumerate() {
                                bytes += f.rgba.len();
                                let color = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba);
                                frames.push(ctx.load_texture(format!("{url}:{i}"), color, egui::TextureOptions::LINEAR));
                                delays.push(f.millis);
                            }
                            self.cache_visual(url, Visual { frames, delays, bytes, animation_loaded: true, last_used: ctx.cumulative_frame_nr() }, 128 * 1024 * 1024);
                        }
                        Err(e) => { self.failed.insert(url, e.to_string()); }
                    }
                }
                Event::Copied(result) => match result {
                    Ok(item) => {
                        let _ = self.store.record_recent(&item.pack_key, &item.sticker.id);
                        if let Some(mut pending) = self.pending.take()
                            && pending.flow.copied().is_ok() {
                            if !pending.interaction.valid() || ctx.input(|i| i.modifiers.any()) { pending.flow.cancel_auto(); }
                            if pending.from_picker && pending.interaction.valid() { self.hide_picker(ctx); }
                            if matches!(pending.flow.hidden(), Ok(Action::Paste)) {
                                if let Some(target) = pending.target { self.paste_at = Some((Instant::now() + Duration::from_millis(180), target, pending.picker)); }
                            } else { self.message("Copied — paste with Ctrl+V / Cmd+V", false); }
                        }
                    }
                    Err(e) => { self.pending = None; self.message(format!("Could not copy: {e:#}"), true); }
                },
                Event::Pasted(result) => match result {
                    Ok(()) => self.message("Copied and paste shortcut sent", false),
                    Err(e) => self.message(format!("Copied — paste manually. {e:#}"), true),
                },
                Event::Login(result) => {
                    self.login_busy = false;
                    match result { Ok(s) => self.message(s, false), Err(e) => self.message(format!("{e:#}"), true) }
                }
                #[cfg(target_os = "linux")]
                Event::Wayland(result) => match result {
                    Ok((w, warning)) => {
                        *self.services.wayland.lock().unwrap() = Some(w);
                        self.message(warning.unwrap_or_else(|| "Wayland keyboard access enabled".into()), false);
                    }
                    Err(e) => self.message(format!("Wayland input unavailable: {e:#}. Stickers can still be copied when a clipboard backend is available."), true),
                },
            }
        }
        if self
            .paste_at
            .as_ref()
            .is_some_and(|(at, _, _)| Instant::now() >= *at)
        {
            let (_, target, picker) = self.paste_at.take().unwrap();
            self.services.paste(target, picker);
        }
    }
}

impl App {
    pub(super) fn cache_visual(&mut self, url: String, visual: Visual, budget: usize) {
        self.visuals.insert(url.clone(), visual);
        while self.visuals.values().map(|v| v.bytes).sum::<usize>() > budget {
            // Shed cold animation frames first. Keep every loaded first frame so
            // ordinary redraws never turn an animated thumbnail into a loader.
            let animation = self
                .visuals
                .iter()
                .filter(|(_, v)| v.frames.len() > 1)
                .min_by_key(|(key, v)| (*key == &url, v.last_used))
                .map(|(key, _)| key.clone());
            if let Some(key) = animation {
                let v = self.visuals.get_mut(&key).unwrap();
                v.frames.truncate(1);
                v.delays.truncate(1);
                let [width, height] = v.frames[0].size();
                v.bytes = width * height * 4;
                v.animation_loaded = false;
            } else {
                // Bound the still cache too when browsing very large libraries.
                let key = self
                    .visuals
                    .iter()
                    .min_by_key(|(key, v)| (*key == &url, v.last_used))
                    .map(|(key, _)| key.clone());
                if let Some(key) = key {
                    self.visuals.remove(&key);
                } else {
                    break;
                }
            }
        }
    }
}
