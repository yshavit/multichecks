use lazy_static::lazy_static;
use regex::Regex;
use std::fmt::Write;
use std::fmt::{Error, Formatter};
use std::io::Read;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::thread::sleep;
use std::time::Duration;
use std::{fmt, io};

const SLEEP_DELAY: u64 = 100;
lazy_static! {
    static ref COLORS_REGEX: Regex =
        Regex::new("\x1b\\[(\\d+)m").expect("Couldn't compile pattern for ASCII color sequences");
}

fn main() {
    let mut commands = Commands::new();
    for line in io::stdin().lines() {
        commands.add_command(line.unwrap());
    }

    let mut terminal = Terminal::new();
    loop {
        commands.summarize_all(&mut terminal);
        sleep(Duration::from_millis(SLEEP_DELAY));
        if commands.all_done() {
            break;
        }
    }
    commands.print_details(&mut terminal);
}

struct Terminal {
    next_write: usize,
    written_lines_lengths: Vec<usize>,
}

impl Terminal {
    fn new() -> Self {
        Terminal {
            next_write: 0,
            written_lines_lengths: Vec::new(),
        }
    }

    fn reset(&mut self) {
        let already_written = self.written_lines_lengths.len();
        if already_written == 0 {
            return;
        }
        for _ in 0..already_written {
            print!("\x1b[2K"); // erase the line
            print!("\x1b[F");
        }
        self.next_write = 0;
    }
}

impl Write for Terminal {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for line in s.split_inclusive("\n") {
            while self.written_lines_lengths.len() < (self.next_write + 1) {
                self.written_lines_lengths.push(0);
            }
            print!("{}", line);
            let prev_len = self
                .written_lines_lengths
                .get_mut(self.next_write)
                .ok_or(Error)?;
            if line.ends_with("\n") {
                *prev_len += line.len() - 1;
                self.next_write += 1;
            } else {
                *prev_len += line.len();
            }
        }
        return Ok(());
    }
}

#[derive(Eq, PartialEq)]
enum CommandStatus {
    Unstarted,
    Running,
    Finished(i32),
    Error(String),
}

#[derive(Copy, Clone, Debug)]
enum Color {
    Normal,
    Gray,
    Green,
    Yellow,
    Red,
    Other(i32),
}

impl Color {
    fn find_all(text: &str) -> Vec<Color> {
        let mut results = Vec::new();
        for captures in COLORS_REGEX.captures_iter(text) {
            let color = match &captures[1] {
                "0" => Color::Normal,
                "90" => Color::Gray,
                "32" => Color::Green,
                "31" => Color::Red,
                "33" => Color::Yellow,
                code => match i32::from_str(code) {
                    Ok(c) => Color::Other(c),
                    Err(_) => Color::Normal,
                },
            };
            results.push(color);
        }
        return results;
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let code = match self {
            Color::Normal => 0,
            Color::Gray => 90,
            Color::Green => 32,
            Color::Red => 31,
            Color::Yellow => 33,
            Color::Other(n) => *n,
        };
        write!(f, "\x1b[{}m", code)
    }
}

impl CommandStatus {
    fn is_terminal_state(&self) -> bool {
        match self {
            CommandStatus::Unstarted | CommandStatus::Running => false,
            CommandStatus::Finished(_) | CommandStatus::Error(_) => true,
        }
    }

    fn is_error(&self) -> bool {
        match self {
            CommandStatus::Unstarted | CommandStatus::Running | CommandStatus::Finished(0) => false,
            _ => true,
        }
    }
}

struct CommandDesc {
    command_strs: Vec<String>,
    command_spawn: Option<std::process::Child>,
    status: CommandStatus,
}

impl CommandDesc {
    const UNSTARTED_DOTS: [&'static str; 4] = ["·  ", " · ", "  ·", " · "];
    const RUNNING_DOTS: [&'static str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    fn new(command: Vec<String>) -> Self {
        Self {
            command_strs: command,
            command_spawn: None,
            status: CommandStatus::Unstarted,
        }
    }

    fn check(&mut self) {
        if self.status.is_terminal_state() {
            return;
        }
        let Some(child) = &mut self.command_spawn else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.status = match status.code() {
                    None => CommandStatus::Error("Error reading status code".to_string()),
                    Some(code) => CommandStatus::Finished(code),
                }
            }
            Ok(None) => {} // nothing
            Err(e) => {
                self.status = CommandStatus::Error(e.to_string());
            }
        }
    }

    fn print_summary(&self, tick: usize, out: &mut Terminal) {
        let (status, color) = match &self.status {
            CommandStatus::Unstarted => (
                Self::UNSTARTED_DOTS[tick % Self::UNSTARTED_DOTS.len()],
                Color::Gray,
            ),
            CommandStatus::Running => (
                Self::RUNNING_DOTS[tick % Self::RUNNING_DOTS.len()],
                Color::Normal,
            ),
            CommandStatus::Finished(0) => ("OK", Color::Green),
            CommandStatus::Finished(_) => ("FAILED", Color::Red),
            CommandStatus::Error(_) => ("FAILED", Color::Red),
        };
        _ = write!(
            out,
            "{}: {}{}\x1b[0m",
            self.command_strs.join(" "),
            color,
            status
        );
    }

    fn print_details(&mut self, out: &mut Terminal) {
        if !self.status.is_error() {
            return;
        }
        match &mut self.command_spawn {
            None => {
                _ = writeln!(
                    out,
                    "{}!{} Failed to start process",
                    Color::Red,
                    Color::Normal
                )
            }
            Some(child) => {
                CommandDesc::print_output(child.stdout.take(), out);
                CommandDesc::print_output(child.stderr.take(), out);
            }
        }
    }

    fn print_output<R: Read>(source: Option<R>, out: &mut Terminal) {
        if let Some(mut contents) = source {
            let mut str: String = String::new();
            match contents.read_to_string(&mut str) {
                Ok(_) => {}
                Err(e) => {
                    _ = write!(
                        &mut str,
                        "{}Error reading stdout{}: {}",
                        Color::Red,
                        Color::Normal,
                        e.to_string()
                    )
                }
            }
            let last_color = Color::Normal;
            if !str.is_empty() {
                for line in str.split("\n") {
                    let colors = Color::find_all(line);
                    let quote_color = match colors.len() {
                        0 => Color::Normal,
                        1 => colors[0],
                        _ => Color::Yellow,
                    };
                    _ = writeln!(out, "{}│{} {}", quote_color, last_color, line);
                }
            }
        }
    }

    fn start(&mut self) {
        let Some((command_name, command_args)) = self.command_strs.split_first() else {
            return
        };
        let mut command = Command::new(command_name);
        command
            .args(command_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.command_spawn = match command.spawn() {
            Ok(child) => {
                self.status = CommandStatus::Running;
                Some(child)
            }
            Err(e) => {
                self.status = CommandStatus::Error(e.to_string());
                None
            }
        }
    }
}

struct Commands {
    commands: Vec<CommandDesc>,
    tick: usize,
}

impl Commands {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
            tick: 0,
        }
    }

    fn add_command(&mut self, text: String) {
        let splits = text
            .split_whitespace()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        self.commands.push(CommandDesc::new(splits));
    }

    fn all_done(&self) -> bool {
        self.commands.iter().all(|c| c.status.is_terminal_state())
    }

    fn summarize_all(&mut self, out: &mut Terminal) {
        out.reset();
        let last_commands_idx = self.commands.len();
        let action: fn(&mut CommandDesc);
        if self.tick > 0 {
            action = CommandDesc::check;
        } else {
            action = CommandDesc::start;
        }
        for command in self.commands.iter_mut() {
            action(command);
        }
        for (i, command) in self.commands.iter().enumerate() {
            command.print_summary(self.tick, out);
            if i != last_commands_idx {
                _ = writeln!(out);
            }
        }
        self.tick = self.tick.wrapping_add(1);
    }

    fn print_details(&mut self, out: &mut Terminal) {
        out.reset();
        for command in &mut self.commands {
            command.print_summary(0, out);
            _ = writeln!(out);
            command.print_details(out);
        }
    }
}
