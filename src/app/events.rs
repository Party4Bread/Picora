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
                "quit" => self.quit = true,
                _ => {}
            }
        }
        while let Ok(event) = self.rx.try_recv() {
            match event {
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
                Event::Preview(result) => {
                    self.browsing = false;
                    match result {
                        Ok(p) => self.preview = Some(p),
                        Err(e) => self.message(format!("{e:#}"), true),
                    }
                }
                Event::Installed(result) => {
                    self.installing = false;
                    match result {
                        Ok(p) => match self.store.save_pack(&p).and_then(|_| self.store.favorite(&p.reference.key(), true)) {
                            Ok(()) => { self.message(format!("Added {} · available offline", p.title), false); self.refresh(); }
                            Err(e) => self.message(e.to_string(), true),
                        },
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
                            while self.visuals.values().map(|v| v.bytes).sum::<usize>() + bytes > 128 * 1024 * 1024 && !self.visuals.is_empty() {
                                let key = self.visuals.keys().next().unwrap().clone();
                                self.visuals.remove(&key);
                            }
                            self.visuals.insert(url, Visual { frames, delays, bytes });
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
                    Ok(w) => {
                        *self.services.wayland.lock().unwrap() = Some(w);
                        self.message("Wayland keyboard access enabled for this session", false);
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
