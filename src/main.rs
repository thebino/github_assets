use adb_client::AdbTcpConnection;
use crossterm::event::{self, Event, KeyCode};
use crossterm::{
    event::KeyEventKind,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::prelude::{Stylize, Terminal};
use ratatui::style::palette::tailwind;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::block::Title;
use ratatui::widgets::{
    BorderType, Clear, Gauge, ListState, Padding, Paragraph, StatefulWidget, Widget,
};
use ratatui::{
    backend::CrosstermBackend,
    widgets::{Block, Borders, List, ListItem},
};

use std::fs::File;
use std::io::{stdout, Result};
use std::net::Ipv4Addr;
use std::path::Path;
use std::{env, io};

mod github;
use github::{download_asset, fetch_releases, Release};

const GAUGE_COLOR: Color = tailwind::GREEN.c800;

/// Indicates if a Release was installed before already.
#[derive(Copy, Clone)]
enum Status {
    Open,
    Installed,
}

struct ReleaseItem<'a> {
    tag_name: &'a str,
    body: &'a str,
    asset_id: i32,
    status: Status,
}

struct StatefulList<'a> {
    state: ListState,
    items: Vec<ReleaseItem<'a>>,
    last_selected: Option<usize>,
    in_progress: Option<usize>,
}

// #[derive(Default)]
struct App<'a> {
    items: StatefulList<'a>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up the terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let terminal = Terminal::new(backend)?;

    // Fetch GitHub releases
    let token = match env::var_os("GH_ACCESS_TOKEN") {
        Some(v) => v.into_string().unwrap(),
        None => panic!("$GH_ACCESS_TOKEN is not set"),
    };
    let owner = env::var_os("GH_OWNER").unwrap().into_string().unwrap();
    let repo = env::var_os("GH_REPO").unwrap().into_string().unwrap();

    let releases = fetch_releases(&owner, &repo, &token)
        .await
        .expect("Could not fetch releases");

    App::new(&releases).run(terminal).await?;

    io::stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

impl Widget for &mut App<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let outer_layout = Layout::vertical([Constraint::Percentage(90), Constraint::Fill(2)]);
        let [top_area, actions_area] = outer_layout.areas(area);

        let inner_layout =
            Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]);
        let [releases_area, info_area] = inner_layout.areas(top_area);

        self.render_releases(releases_area, buf);
        self.render_info(info_area, buf);
        self.render_actions(actions_area, buf);

        if self.items.in_progress.is_some() {
            self.render_popup(top_area, buf);
        }
    }
}

impl App<'_> {
    fn render_releases(&mut self, area: Rect, buf: &mut Buffer) {
        // Convert releases to ListItems
        let items: Vec<ListItem> = self
            .items
            .items
            .iter()
            .map(|r| ListItem::new(r.tag_name.to_string()))
            .collect();

        // releases
        let list = List::new(items.clone())
            .block(
                Block::default()
                    .title("GitHub Releases")
                    .borders(Borders::ALL),
            )
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol("► ");

        StatefulWidget::render(list, area, buf, &mut self.items.state);
    }

    fn render_info(&mut self, area: Rect, buf: &mut Buffer) {
        let info = if let Some(i) = self.items.state.selected() {
            self.items.items[i].body.to_string()
        } else {
            "Select a release on the left side to see its description here...".to_string()
        };

        Paragraph::new(info)
            .block(Block::new().borders(Borders::ALL))
            .bold()
            .render(area, buf);
    }

    fn render_popup(&mut self, area: Rect, buf: &mut Buffer) {
        let popup_layout = Layout::vertical([
            Constraint::Percentage((100 - 20) / 2),
            Constraint::Percentage(20),
            Constraint::Percentage((100 - 20) / 2),
        ])
        .split(area);

        let popup_area = Layout::horizontal([
            Constraint::Percentage((100 - 60) / 2),
            Constraint::Percentage(60),
            Constraint::Percentage((100 - 60) / 2),
        ])
        .split(popup_layout[1])[1];

        Clear.render(popup_area, buf);
        let title = Title::from("Progress").alignment(Alignment::Center);
        let title = Block::new()
            .borders(Borders::NONE)
            .padding(Padding::vertical(1))
            .title(title);

        // TODO: get a real progress?
        Gauge::default()
            .block(title)
            .gauge_style(GAUGE_COLOR)
            .percent(100u16)
            .render(popup_area, buf);
        Block::bordered()
            .borders(Borders::NONE)
            .title("Progress")
            .render(popup_area, buf);
    }

    fn render_actions(&mut self, area: Rect, buf: &mut Buffer) {
        // actions
        let actions: Line = vec![
            Span::styled("↓↑".to_string(), Style::default().fg(Color::LightBlue)),
            " to move ".into(),
            Span::styled("←".to_string(), Style::default().fg(Color::LightBlue)),
            " to unselect ".into(),
            Span::styled("→".to_string(), Style::default().fg(Color::LightBlue)),
            " to change status ".into(),
            Span::styled("g/G".to_string(), Style::default().fg(Color::LightBlue)),
            " to go to top/bottom ".into(),
            Span::styled("q".to_string(), Style::default().fg(Color::LightBlue)),
            " to quit ".into(),
        ]
        .into();

        Paragraph::new(actions)
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .centered()
            .render(area, buf);
    }
    async fn run(&mut self, mut terminal: Terminal<impl Backend>) -> io::Result<()> {
        loop {
            self.draw(&mut terminal)?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    use KeyCode::*;
                    match key.code {
                        Char('q') | Esc => return Ok(()),
                        Char('h') | Left => self.items.unselect(),
                        Char('j') | Down => self.items.next(),
                        Char('k') | Up => self.items.previous(),
                        Char('l') | Right | Enter => self.flip_status(),
                        Char('g') => self.go_top(),
                        Char('G') => self.go_bottom(),
                        _ => {}
                    }
                }
            }

            // TODO: install selected apk
            if let Some(index) = self.items.in_progress {
                if self.items.items[index].asset_id == -1 {
                    println!("No APK asset found in the selected release.");
                } else {
                    let asset_id = self.items.items[index].asset_id;

                    let apk_path = "/tmp/app.apk";

                    let token = match env::var_os("GH_ACCESS_TOKEN") {
                        Some(v) => v.into_string().unwrap(),
                        None => panic!("$GH_ACCESS_TOKEN is not set"),
                    };
                    let owner = env::var_os("GH_OWNER").unwrap().into_string().unwrap();
                    let repo = env::var_os("GH_REPO").unwrap().into_string().unwrap();

                    let download_result =
                        download_asset(&owner, &repo, &token, asset_id, apk_path).await;

                    match download_result {
                        Ok(_) => {
                            // create an ADB connection to the device
                            let mut connection =
                                AdbTcpConnection::new(Ipv4Addr::from([127, 0, 0, 1]), 5037)
                                    .unwrap();

                            let mut input = File::open(Path::new(&apk_path)).unwrap();
                            let send_result = connection.send(
                                None::<String>,
                                &mut input,
                                "/data/local/tmp/app.apk",
                            );

                            match send_result {
                                Ok(_) => {
                                    // TODO: handle result
                                    let install_result = connection.shell_command(
                                        &None,
                                        vec!["pm", "install", "-r", "/data/local/tmp/app.apk"],
                                    );

                                    match install_result {
                                        Ok(_) => {
                                            //
                                            self.items.in_progress = None;
                                        }
                                        Err(error) => {
                                            println!("Could not install apk on device! {}", error);
                                            self.items.in_progress = None;
                                        }
                                    }
                                }
                                Err(error) => {
                                    println!("Could not send apk to device! {}", error)
                                }
                            }
                        }
                        Err(error) => println!("Could not download apk from github! {}", error),
                    }
                };
            }
        }
    }

    fn draw(&mut self, terminal: &mut Terminal<impl Backend>) -> io::Result<()> {
        terminal.draw(|f| f.render_widget(self, f.size()))?;
        Ok(())
    }
}

impl<'a> App<'a> {
    fn new(releases: &'a [Release]) -> Self {
        Self {
            items: StatefulList {
                state: ListState::default(),
                items: releases.iter().map(ReleaseItem::from).collect(),
                last_selected: None,
                in_progress: None,
            },
        }
    }
    /// Changes the status of the selected list item
    fn flip_status(&mut self) {
        if let Some(i) = self.items.state.selected() {
            self.items.in_progress = Some(i);
            self.items.items[i].status = match self.items.items[i].status {
                Status::Installed => Status::Open,
                Status::Open => Status::Installed,
            }
        }
    }

    fn go_top(&mut self) {
        self.items.state.select(Some(0));
    }

    fn go_bottom(&mut self) {
        self.items.state.select(Some(self.items.items.len() - 1));
    }
}

impl StatefulList<'_> {
    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => self.last_selected.unwrap_or(0),
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.last_selected.unwrap_or(0),
        };
        self.state.select(Some(i));
    }

    fn unselect(&mut self) {
        let offset = self.state.offset();
        self.last_selected = self.state.selected();
        self.state.select(None);
        *self.state.offset_mut() = offset;
    }
}

impl<'a> From<&'a Release> for ReleaseItem<'a> {
    fn from(release: &'a github::Release) -> Self {
        let download_url =
            if let Some(asset) = release.assets.iter().find(|a| a.name.ends_with(".apk")) {
                asset.id
            } else {
                -1i32
            };

        Self {
            tag_name: &release.tag_name,
            body: &release.body,
            asset_id: download_url,
            status: Status::Open,
        }
    }
}
