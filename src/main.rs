use clap::Parser;
use dirs::data_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::OpenOptions;
use std::hash::Hash;
use std::io::prelude::*;
use std::io::{Read, Write};
use std::path::Path;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::{fs, io};
use ter_menu::TerminalDropDown;

fn get_default_path() -> String {
    data_dir()
        .unwrap()
        .join("todo.todo")
        .to_str()
        .unwrap()
        .to_string()
}

fn fix(path: String) -> String {
    let path = Path::new(&path);
    if path.is_dir() || !path.extension().map_or(false, |ext| ext == "todo") {
        let mut new_path = path.to_path_buf();
        if new_path.is_dir() {
            new_path.push("todo.todo");
        } else {
            new_path.set_extension("todo");
        }
        new_path.to_str().unwrap().to_string()
    } else {
        path.to_str().unwrap().to_string()
    }
}

fn exit_when_refuse() {
    let stdin = io::stdin();
    print!("Are you sure?(y/N)");
    io::stdout().flush().unwrap();
    let mut buffer = String::new();
    stdin.read_line(&mut buffer).unwrap();
    if buffer.to_lowercase().trim() != "y" {
        println!("Canceled.");
        exit(0);
    }
}

fn open_todo_list(path: String) -> TodoList {
    let path = fix(path);
    TodoList::open_without_doubt(path.as_str())
}

#[derive(Parser, Debug)]
#[command(name = "Todo", version, about, long_about = None)]
enum Command {
    Add {
        #[arg(short, long, default_value_t = String::from("Untitled"))]
        name: String,
        #[arg(short, long, default_value_t = 0)]
        priority: i16, // 优先级
        #[arg(long, default_value_t = get_default_path())]
        path: String,
        content: String,
    },
    View {
        #[arg(long, default_value_t = get_default_path())]
        path: String,
    },
    Find {
        #[arg(long, default_value_t = get_default_path())]
        path: String,

        name: String,
    },
    Clear {
        #[arg(long, default_value_t = get_default_path())]
        path: String,
    },
    Delete {
        #[arg(long, default_value_t = get_default_path())]
        path: String,

        name: String,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Hash)]
struct TodoItem {
    name: String,
    content: String,
    priority: i16,
}

impl Display for TodoItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Item: {} \nContent: {} \n(Priority: {})",
            self.name, self.content, self.priority
        )
    }
}

struct TodoList {
    buffer: Vec<TodoItem>,
    file: Mutex<fs::File>,
}

impl TodoList {
    fn add_item(&mut self, item: TodoItem) {
        self.buffer.push(item);
    }

    fn analysis(&self) -> &Vec<TodoItem> {
        &self.buffer
    }

    fn clear(&mut self) -> Result<(), Box<dyn Error>> {
        let mut file = self.file.lock().unwrap();

        // 步骤1：先刷新缓冲区，避免数据残留
        file.flush()?;

        // 步骤2：截断文件为 0 字节（物理清空文件）
        file.set_len(0)?;

        // 步骤3：重置指针到开头，确保后续写入从正确位置开始
        file.rewind()?;

        // 步骤4：同步清空内存中的 buffer（关键！否则 Drop 时会写回旧数据）
        self.buffer.clear();

        Ok(())
    }

    fn del_by_name(&mut self, name: String) {
        let mut index = usize::MAX;
        let todo_item = self.get_item_by_name(name.as_str()).unwrap();
        for (i, item) in self.buffer.iter().enumerate() {
            if item == todo_item {
                index = i;
                break;
            }
        }

        if index != usize::MAX {
            self.buffer.remove(index);
        }
    }

    fn save_to_file(&self) -> Result<(), Box<dyn Error>> {
        let serialized = serde_json::to_string(&self.buffer)?;
        let mut file = self.file.lock().unwrap();
        file.set_len(0)?; // 用 ? 替代 unwrap()
        file.rewind()?;
        file.write_all(serialized.as_bytes())?;
        Ok(())
    }

    fn find_items_by_name(&self, keyword: &str) -> Vec<&TodoItem> {
        let keyword_lower = keyword.to_lowercase();
        self.buffer
            .iter()
            // 匹配规则：名称（小写）包含关键词（小写），覆盖更多场景
            .filter(|item| item.name.to_lowercase().contains(&keyword_lower))
            .collect()
    }

    // 保留原方法（如需单个结果可调用此方法，基于新的匹配规则）
    fn get_item_by_name(&self, keyword: &str) -> Option<&TodoItem> {
        self.find_items_by_name(keyword).into_iter().next()
    }

    fn open(value: &str) -> Result<Self, Box<dyn Error>> {
        // 打开文件（只读、可写、不存在则创建）
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(value)
            .map_err(|e| format!("无法打开文件: {}", e))?; // 更明确的错误提示

        // 确保文件指针在开头
        file.rewind()?;

        // 读取文件内容（使用 ? 处理错误，而不是 unwrap）
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("读取文件失败: {}", e))?;

        // 解析 JSON（处理空内容或有效内容）
        let buffer: Vec<TodoItem> = if content.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&content)
                .map_err(|e| format!("JSON 解析失败: {} (内容: {})", e, content))?
        };

        Ok(TodoList {
            buffer,
            file: Mutex::new(file),
        })
    }

    fn open_without_doubt(value: &str) -> Self {
        Self::open(value).unwrap_or_else(|e| {
            println!("The formatting of file is invalid. \n {}", e);
            exit(1);
        })
    }
}

impl Default for TodoList {
    fn default() -> Self {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(get_default_path())
            .unwrap();
        file.set_len(0).unwrap();
        file.rewind().unwrap();

        TodoList {
            buffer: Vec::new(),
            file: Mutex::new(file),
        }
    }
}

impl Drop for TodoList {
    fn drop(&mut self) {
        if let Err(e) = self.save_to_file() {
            eprintln!("保存文件失败: {}", e);
        }
    }
}

fn main() {
    let args = Command::parse();
    match args {
        Command::Add {
            name,
            content,
            priority,
            path,
        } => {
            let todo_item = TodoItem {
                name,
                content,
                priority,
            };
            let mut todo_list = open_todo_list(path);
            todo_list.add_item(todo_item);
        }
        Command::View { path } => {
            let todo_list = open_todo_list(path);
            let mut buffer = todo_list.analysis().clone();
            buffer.sort_by(|a, b| b.priority.cmp(&a.priority));
            buffer.iter().for_each(|item| {
                println!("--------------------\n{}\n--------------------", item);
            });
        }
        Command::Find { path, name } => {
            let todo_list = open_todo_list(path);
            let found = todo_list.find_items_by_name(&name[..]);
            if found.len() == 0 {
                println!("No item with that name found");
                return;
            }
            found.iter().for_each(|x| {
                println!("--------------------\n{}\n--------------------", x);
            })
        }
        Command::Clear { path } => {
            exit_when_refuse();
            let mut todo_list = open_todo_list(path);
            todo_list.clear().unwrap_or_else(|e| {
                eprintln!("There is something wrong. {}", e);
                exit(1);
            });
            println!("Done.");
        }
        Command::Delete { path, name } => {
            // 关键：TodoList 全程用 Arc<Mutex<>> 包装，确保 'static 生命周期
            let todo_list = Arc::new(Mutex::new(open_todo_list(path)));
            // 临时解锁读取匹配项，避免锁与 todo_list 生命周期绑定（解决 `list` 生命周期错误）
            let todos: Vec<TodoItem> = {
                let list_guard = todo_list.lock().unwrap(); // 临时锁
                list_guard
                    .find_items_by_name(&name[..])
                    .iter()
                    .copied()
                    .cloned() // 克隆 TodoItem，脱离锁的生命周期
                    .collect()
            }; // 此处 list_guard 自动释放锁，避免生命周期问题

            if todos.is_empty() {
                println!("No item with that name found.");
                return;
            }

            // 为每个待选项创建独立闭包（每个闭包克隆 Arc，满足 'static）
            let mut drop_down_items = HashMap::new();
            for todo in todos {
                let list_clone = todo_list.clone(); // 克隆 Arc，每个闭包独立持有
                drop_down_items.insert(todo.clone(), move |_selected: &TodoItem| {
                    // 解锁执行删除（Arc 克隆确保生命周期足够）
                    let mut list_guard = list_clone.lock().unwrap();
                    list_guard.del_by_name(todo.name.clone());
                    println!("\nSuccessfully deleted item: {}", todo.name);
                });
            }

            // 启动下拉菜单并等待线程结束（确保生命周期匹配）
            println!(
                "Found {} matching items. Use Up/Down to select, Enter to delete, Esc to cancel.",
                drop_down_items.len()
            );
            let dropdown = TerminalDropDown::use_drop_down(drop_down_items, 1);
            if let Err(e) = dropdown.wait() {
                eprintln!("Error during selection: {:?}", e);
            }

            println!("\nDelete command finished.");
        }
    }
}
