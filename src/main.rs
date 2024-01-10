use regex::{self, Regex};
use std::env;
use std::fs::File;
use std::io::{self, BufRead, Write};

#[derive(Debug)]
struct TodoItem {
    name: String,
    done: bool,
}

#[derive(Debug)]
enum ParseTodosError {
    RegexError(regex::Error),
    IoError(io::Error),
    InvalidSyntax(String),
}

impl From<regex::Error> for ParseTodosError {
    fn from(err: regex::Error) -> Self {
        Self::RegexError(err)
    }
}

impl From<io::Error> for ParseTodosError {
    fn from(err: io::Error) -> Self {
        Self::IoError(err)
    }
}

#[derive(Debug)]
enum ParseIdsError {
    InvalidId(usize),
    ParseError(std::num::ParseIntError),
}

impl From<std::num::ParseIntError> for ParseIdsError {
    fn from(err: std::num::ParseIntError) -> Self {
        Self::ParseError(err)
    }
}

#[derive(Debug)]
enum Command {
    LIST,
    ADD,
    DONE,
    UNDO,
    REMOVE,
    HELP,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut command = Command::LIST;

    if args.len() > 1 {
        command = match args[1].to_lowercase().as_str() {
            "add" => Command::ADD,
            "list" => Command::LIST,
            "done" => Command::DONE,
            "undo" => Command::UNDO,
            "remove" => Command::REMOVE,
            "help" | "-h" | "--help" => Command::HELP,
            _ => {
                eprintln!(
                    "invalid command: {}\nUse '{} help' for help",
                    args[1], args[0]
                );
                return;
            }
        }
    }

    if let Command::HELP = command {
        println!("Usage: {} [command]", args[0]);
        println!("Commands:");
        println!("  list: list all todo items (same as no argument)");
        println!("  add [items]: add items to the todo list");
        println!("  done [item ids]: mark todo items as done");
        println!("  undo [item ids]: unmark todo items as done");
        println!("  remove [item ids]: remove todo items from the list");
        println!("  help: print this help message");
        println!("\nBy default the items are stored in $HOME/.todo.md");
        println!("This can be changed by setting the environment variable TODO_FILE");
        return;
    }

    let file_name = env::var("TODO_FILE").unwrap_or(format!(
        "{}/.todo.md",
        env::var("HOME").expect("HOME env variable should be set")
    ));

    let file = match File::options().read(true).open(&file_name) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening file '{}': {}", file_name, e.to_string());
            return;
        }
    };

    let mut todos = match parse_todos(&file) {
        Ok(v) => v,
        Err(ParseTodosError::IoError(e)) => {
            eprintln!("Error reading file: {}", e.to_string());
            return;
        }
        Err(ParseTodosError::RegexError(e)) => {
            eprintln!("An unexprected regex error occured: {}", e.to_string());
            return;
        }
        Err(ParseTodosError::InvalidSyntax(line)) => {
            eprintln!("Invalid syntax detected: \"{}\"", line);
            return;
        }
    };

    match command {
        Command::LIST => {
            print_todos(&todos);
            return;
        }
        Command::ADD => {
            if args.len() < 3 {
                eprintln!("No items to add");
                return;
            }
            for i in 2..args.len() {
                todos.push(TodoItem {
                    name: args[i].to_string(),
                    done: false,
                });
            }
        }
        Command::DONE | Command::UNDO | Command::REMOVE => {
            if args.len() < 3 {
                eprintln!("No item ids given");
                return;
            }
            let mut ids: Vec<usize> = match args
                .get(2..args.len())
                .unwrap()
                .iter()
                .map(|arg| -> Result<usize, ParseIdsError> {
                    match arg.parse::<usize>() {
                        Err(e) => Err(ParseIdsError::from(e)),
                        Ok(x) => {
                            if x > todos.len() {
                                Err(ParseIdsError::InvalidId(x))
                            } else {
                                Ok(x)
                            }
                        }
                    }
                })
                .collect::<Result<Vec<usize>, ParseIdsError>>()
            {
                Ok(v) => v,
                Err(ParseIdsError::ParseError(e)) => {
                    eprintln!("Error parsing id: {}", e.to_string());
                    return;
                }
                Err(ParseIdsError::InvalidId(id)) => {
                    dbg!(&todos);
                    println!("Invalid id {}", id);
                    return;
                }
            };

            // sort ids descending and remove duplicates so that removing doesn't cause any issues
            ids.sort_by(|a, b| b.cmp(a));
            ids.dedup();

            for id in ids {
                match command {
                    Command::DONE => {
                        todos[id - 1].done = true;
                    }
                    Command::UNDO => {
                        todos[id - 1].done = false;
                    }
                    Command::REMOVE => {
                        todos.remove(id - 1);
                    }
                    Command::LIST | Command::ADD | Command::HELP => {
                        panic!("Should not be possible")
                    }
                }
            }
        }
        Command::HELP => panic!("This should have been handled earlier"),
    }

    if let Err(e) = wirte_todos_to_file(&file_name, &todos) {
        eprintln!("Error writing to file: {}", e.to_string());
        return;
    }
    print_todos(&todos);
}

fn parse_todos(file: &File) -> Result<Vec<TodoItem>, ParseTodosError> {
    io::BufReader::new(file)
        .lines()
        .map(|l| -> Result<TodoItem, ParseTodosError> {
            match l {
                Err(e) => Err(ParseTodosError::from(e)),
                Ok(line) => {
                    let r = Regex::new(r"\- \[([ X])\] (.*)")?;
                    let caps = r
                        .captures(&line)
                        .ok_or(ParseTodosError::InvalidSyntax(line.to_string()))?;

                    let done = caps
                        .get(1)
                        .ok_or(ParseTodosError::InvalidSyntax(line.to_string()))?
                        .as_str()
                        == "X";

                    let name = caps
                        .get(2)
                        .ok_or(ParseTodosError::InvalidSyntax(line.to_string()))?
                        .as_str()
                        .to_string();

                    Ok(TodoItem { name, done })
                }
            }
        })
        .collect()
}

fn wirte_todos_to_file(file_name: &String, todos: &Vec<TodoItem>) -> io::Result<()> {
    let mut file = File::options()
        .write(true)
        .truncate(true)
        .open(&file_name)?;

    for todo in todos {
        file.write_fmt(format_args!(
            "- [{}] {}\n",
            if todo.done { "X" } else { " " },
            todo.name
        ))?;
    }
    Ok(())
}

fn print_todos(todos: &Vec<TodoItem>) {
    for (idx, todo) in todos.iter().enumerate() {
        if todo.done {
            println!("{}.\t\x1b[9m{}\x1b[m", idx + 1, todo.name);
        } else {
            println!("{}.\t{}", idx + 1, todo.name);
        }
    }
}
