//! DOS Games — Navigateur et lanceur de jeux DOS via DOSBox.
//!
//! Scanne récursivement un dossier (par défaut `~/Documents`, configurable via
//! la variable d'environnement `DOSGAMES_DIR`) à la recherche de fichiers
//! `.IMA` / `.IMG` / `.EXE` et les lance avec DOSBox.
//!
//! Exemple : `/home/omar0/Documents/dukenukem/Duke Nukum.IMA`

use zer0fun_api::{
    BorderStyle, Buffer, Category, Color, Context, Event, FunModule, KeyEvent, Meta, Permissions,
    Rect, Style,
};

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Profondeur maximale du scan récursif.
const MAX_DEPTH: u32 = 4;

/// Tailles standard des images disquette (360K, 720K, 1.2M, 1.44M, 2.88M).
const FLOPPY_SIZES: [u64; 5] = [368_640, 737_280, 1_228_800, 1_474_560, 2_949_120];

#[derive(Clone, Copy, PartialEq, Eq)]
enum GameKind {
    Exe,
    Floppy,
    Hdd,
}

impl GameKind {
    fn tag(self) -> &'static str {
        match self {
            GameKind::Exe => "EXE",
            GameKind::Floppy => "IMG",
            GameKind::Hdd => "HDD",
        }
    }
}

struct GameEntry {
    name: String,
    path: PathBuf,
    kind: GameKind,
    size: u64,
}

pub struct DosGames {
    games_dir: PathBuf,
    games: Vec<GameEntry>,
    selected: usize,
    scroll: usize,
    view_h: usize,
    status: String,
    status_ok: bool,
    tick: u64,
}

impl DosGames {
    /// Rescanne le dossier de jeux.
    fn rescan(&mut self) {
        let mut games = Vec::new();
        let dir = self.games_dir.clone();
        scan_dir(&dir, 0, &mut games);
        games.sort_by_key(|g| g.name.to_lowercase());
        games.dedup_by(|a, b| a.path == b.path);
        self.games = games;
        self.selected = 0;
        self.scroll = 0;
        if self.games.is_empty() {
            self.status = format!("Aucun jeu trouve dans {}", self.games_dir.display());
            self.status_ok = false;
        } else {
            self.status = format!("{} jeu(x) trouve(s) — bon jeu !", self.games.len());
            self.status_ok = true;
        }
    }

    /// Déplace la sélection avec wrap-around et ajuste le scroll.
    fn move_sel(&mut self, delta: i64) {
        if self.games.is_empty() {
            return;
        }
        let n = self.games.len() as i64;
        self.selected = (self.selected as i64 + delta).rem_euclid(n) as usize;
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        if self.view_h > 0 && self.selected >= self.scroll + self.view_h {
            self.scroll = self.selected + 1 - self.view_h;
        }
    }

    /// Lance le jeu sélectionné avec DOSBox.
    fn launch_selected(&mut self) {
        let Some(game) = self.games.get(self.selected) else {
            self.status = String::from("Rien a lancer — appuie sur 'r' pour scanner");
            self.status_ok = false;
            return;
        };
        match launch(game) {
            Ok(()) => {
                self.status = format!("> {} lance dans DOSBox !", game.name);
                self.status_ok = true;
            }
            Err(e) => {
                self.status = e;
                self.status_ok = false;
            }
        }
    }

    /// Synchronise la hauteur visible avec le terminal.
    fn sync_view(&mut self, ctx: &Context) {
        self.view_h = (ctx.term_height as usize).saturating_sub(5);
        if self.view_h > 0 && self.selected >= self.scroll + self.view_h {
            self.scroll = self.selected + 1 - self.view_h;
        }
    }
}

/// Dossier de jeux par défaut : `$DOSGAMES_DIR`, sinon `~/Documents`, sinon `~`.
fn default_games_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DOSGAMES_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        let docs = home.join("Documents");
        if docs.is_dir() {
            return docs;
        }
        return home;
    }
    PathBuf::from(".")
}

/// Scan récursif (profondeur limitée, ignore les fichiers cachés).
fn scan_dir(dir: &Path, depth: u32, out: &mut Vec<GameEntry>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else { continue };
        if name_str.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if meta.is_dir() {
            scan_dir(&path, depth + 1, out);
        } else if meta.is_file() {
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            let ext = ext.to_ascii_lowercase();
            let kind = match ext.as_str() {
                "exe" | "com" | "bat" => GameKind::Exe,
                "ima" | "img" => {
                    if FLOPPY_SIZES.contains(&meta.len()) {
                        GameKind::Floppy
                    } else {
                        GameKind::Hdd
                    }
                }
                _ => continue,
            };
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(name_str)
                .replace('_', " ");
            out.push(GameEntry {
                name,
                path,
                kind,
                size: meta.len(),
            });
        }
    }
}

/// Lance un jeu via DOSBox (processus détaché, sorties silencieuses).
fn launch(game: &GameEntry) -> Result<(), String> {
    let mut cmd = Command::new("dosbox");
    match game.kind {
        // DOSBox monte automatiquement le dossier parent et exécute le fichier.
        GameKind::Exe => {
            cmd.arg(&game.path);
        }
        // Image disquette bootable : imgmount + boot a:
        GameKind::Floppy => {
            cmd.arg("-c")
                .arg(format!("imgmount a \"{}\" -t floppy", game.path.display()))
                .arg("-c")
                .arg("boot a:");
        }
        // Image disque dur bootable : imgmount + boot c:
        GameKind::Hdd => {
            cmd.arg("-c")
                .arg(format!("imgmount c \"{}\" -t hdd", game.path.display()))
                .arg("-c")
                .arg("boot c:");
        }
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            String::from("DOSBox introuvable ! Installe-le : sudo apt install dosbox")
        }
        _ => format!("Echec du lancement : {}", e),
    })
}

fn format_size(bytes: u64) -> String {
    const MIB: u64 = 1_048_576;
    const KIB: u64 = 1_024;
    if bytes >= MIB {
        format!("{:.1}M", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{}K", bytes / KIB)
    } else {
        format!("{}B", bytes)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

impl FunModule for DosGames {
    fn meta() -> Meta {
        Meta::new(
            "dosgames",
            "0.1.0",
            "Lanceur de jeux DOS via DOSBox (.IMA / .IMG / .EXE)",
        )
        .category(Category::Game)
        // Si Permissions utilise un builder : Permissions::none().fs_read(true).process(true)
        .permissions(Permissions {
            fs_read: true,
            process: true,
            ..Permissions::none()
        })
    }

    fn new(_ctx: Context) -> Self {
        let mut module = DosGames {
            games_dir: default_games_dir(),
            games: Vec::new(),
            selected: 0,
            scroll: 0,
            view_h: 10,
            status: String::from("Scan des jeux en cours…"),
            status_ok: true,
            tick: 0,
        };
        module.rescan();
        module
    }

    fn on_mount(&mut self, ctx: &mut Context) {
        self.sync_view(ctx);
        ctx.request_redraw();
    }

    fn on_event(&mut self, ev: Event, ctx: &mut Context) -> bool {
        self.sync_view(ctx);
        match ev {
            Event::Key(KeyEvent::Esc) | Event::Key(KeyEvent::Char('q')) => {
                ctx.quit();
                true
            }
            Event::Key(KeyEvent::Up) | Event::Key(KeyEvent::Char('k')) => {
                self.move_sel(-1);
                ctx.request_redraw();
                true
            }
            Event::Key(KeyEvent::Down) | Event::Key(KeyEvent::Char('j')) => {
                self.move_sel(1);
                ctx.request_redraw();
                true
            }
            Event::Key(KeyEvent::Enter) => {
                self.launch_selected();
                ctx.request_redraw();
                true
            }
            Event::Key(KeyEvent::Char('r')) | Event::Key(KeyEvent::Char('R')) => {
                self.rescan();
                ctx.request_redraw();
                true
            }
            Event::Resize { .. } => {
                ctx.request_redraw();
                true
            }
            _ => false,
        }
    }

    fn on_tick(&mut self, ctx: &mut Context) {
        self.tick = self.tick.wrapping_add(1);
        self.sync_view(ctx);
        // Redessine seulement pour le clignotement du curseur DOS.
        if self.tick % 2 == 0 {
            ctx.request_redraw();
        }
    }

    fn draw(&self, area: Rect, buf: &mut Buffer) {
        buf.fill(0, 0, area.width, area.height, ' ');
        buf.draw_border(BorderStyle::Rounded);

        if area.width < 30 || area.height < 8 {
            buf.set_string_styled(
                1,
                1,
                "Terminal trop petit !",
                Style { fg: Some(Color::Red), ..Default::default() },
            );
            return;
        }

        let x = 2;
        let w = area.width.saturating_sub(4) as usize;
        let right = area.width - 2;

        // ── En-tête : dossier scanné + compteur ─────────────────────
        let dir_str = truncate(
            &format!("Dir: {}", self.games_dir.display()),
            w.saturating_sub(10),
        );
        buf.set_string_styled(x, 1, &dir_str, Style { fg: Some(Color::Cyan), ..Default::default() });

        let count = format!("{} jeu(x)", self.games.len());
        let count_x = right.saturating_sub(count.len() as u16);
        buf.set_string_styled(
            count_x,
            1,
            &count,
            Style { fg: Some(Color::Yellow), ..Default::default() },
        );

        // ── Liste des jeux ──────────────────────────────────────────
        let list_y = 2;
        let list_h = area.height.saturating_sub(5) as usize;

        if self.games.is_empty() {
            // État vide avec logo ASCII DOS
            let logo = [
                " ____   ___  ____  ",
                "|  _ \\ / _ \\/ ___| ",
                "| | | | | | \\___ \\ ",
                "| |_| | |_| |___) |",
                "|____/ \\___/|____/ ",
            ];
            let msgs: [(&str, Color); 4] = [
                ("Aucun jeu DOS trouve", Color::Yellow),
                ("Formats : .IMA  .IMG  .EXE", Color::White),
                ("Astuce : export DOSGAMES_DIR=/chemin/vers/jeux", Color::Rgb(130, 130, 130)),
                ("'r' pour rescanner", Color::Rgb(130, 130, 130)),
            ];
            let show_logo = list_h >= logo.len() + msgs.len() + 3;
            let total = if show_logo { logo.len() + 1 + msgs.len() } else { msgs.len() };
            let mut ly = list_y + ((list_h as u16).saturating_sub(total as u16) / 2);
            if show_logo {
                for line in logo {
                    let lw = line.chars().count() as u16;
                    let lx = (area.width.saturating_sub(lw)) / 2;
                    buf.set_string_styled(lx, ly, line, Style { fg: Some(Color::Green), bold: true, ..Default::default() });
                    ly += 1;
                }
                ly += 1;
            }
            for (text, color) in msgs {
                let tw = text.chars().count() as u16;
                let tx = (area.width.saturating_sub(tw)) / 2;
                buf.set_string_styled(tx, ly, text, Style { fg: Some(color), ..Default::default() });
                ly += 1;
            }
        } else {
            let start = self.scroll.min(self.games.len());
            let end = (start + list_h).min(self.games.len());
            for (row, idx) in (start..end).enumerate() {
                let gy = list_y + row as u16;
                let game = &self.games[idx];
                let sel = idx == self.selected;

                let tag = format!("{} {}", game.kind.tag(), format_size(game.size));
                let tag_len = tag.len() as u16;
                let name_max = w.saturating_sub(tag.len() + 4);
                let name = truncate(&game.name, name_max);

                let line = if sel {
                    format!("> {}", name)
                } else {
                    format!("  {}", name)
                };
                let padded = format!("{:<width$}", line, width = w);

                let line_style = if sel {
                    Style { fg: Some(Color::Black), bg: Some(Color::Green), bold: true, ..Default::default() }
                } else {
                    Style { fg: Some(Color::White), ..Default::default() }
                };
                buf.set_string_styled(x, gy, &truncate(&padded, w), line_style);

                let tag_style = if sel {
                    Style { fg: Some(Color::Black), bg: Some(Color::Green), bold: true, ..Default::default() }
                } else {
                    Style { fg: Some(Color::Yellow), ..Default::default() }
                };
                buf.set_string_styled(right.saturating_sub(tag_len), gy, &tag, tag_style);
            }

            // Indicateurs de scroll sur la bordure droite
            let border_x = area.width - 1;
            if start > 0 {
                buf.set_string_styled(border_x, list_y, "^", Style { fg: Some(Color::Cyan), ..Default::default() });
            }
            if end < self.games.len() {
                buf.set_string_styled(
                    border_x,
                    list_y + list_h as u16 - 1,
                    "v",
                    Style { fg: Some(Color::Cyan), ..Default::default() },
                );
            }
        }

        // ── Ligne de statut style invite DOS ────────────────────────
        let status_y = area.height - 3;
        let cursor = if (self.tick / 2) % 2 == 0 { "█" } else { " " };
        let prompt = truncate(&format!("C:\\> {}{}", self.status, cursor), w);
        let prompt_color = if self.status_ok { Color::Green } else { Color::Red };
        buf.set_string_styled(x, status_y, &prompt, Style { fg: Some(prompt_color), ..Default::default() });

        // ── Barre d'aide ────────────────────────────────────────────
        let help = "up/down ou j/k : naviguer | Enter : lancer | r : rescan | Esc : quitter";
        let help_y = area.height - 2;
        buf.set_string_styled(
            x,
            help_y,
            &truncate(help, w),
            Style { fg: Some(Color::Rgb(120, 120, 120)), ..Default::default() },
        );
    }
}

zer0fun_api::register!(DosGames);