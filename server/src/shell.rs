//! 模拟 shell：每台 VM 一份（内存虚拟文件系统 + 工作目录）。
//!
//! 支持的命令：pwd / ls / mkdir / cd / echo <text> [> file | >> file] / cat <file>。
//! 纯内存态，随进程存活——demo 尺度足够；真实系统里这里是 guest 的串口另一端。

use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub struct Dir {
    entries: BTreeMap<String, Node>,
}

#[derive(Clone)]
pub enum Node {
    Dir(Dir),
    File(String),
}

pub struct Shell {
    /// 从根到当前目录的路径段
    cwd: Vec<String>,
    root: Node,
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    pub fn new() -> Self {
        let mut root = Node::Dir(Dir::default());
        if let Node::Dir(dir) = &mut root {
            dir.entries.insert(
                "README.txt".into(),
                Node::File(
                    "simulated shell: pwd | ls | mkdir <dir> | cd <path> | \
                     echo <text> [> file | >> file] | cat <file>"
                        .into(),
                ),
            );
            dir.entries.insert("home".into(), Node::Dir(Dir::default()));
            let mut var = Dir::default();
            var.entries.insert("log".into(), Node::Dir(Dir::default()));
            dir.entries.insert("var".into(), Node::Dir(var));
        }
        Self {
            cwd: Vec::new(),
            root,
        }
    }

    /// 提示符：vm1:/a/b$
    pub fn prompt(&self, vm_id: u64) -> String {
        let path = if self.cwd.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.cwd.join("/"))
        };
        format!("vm{vm_id}:{path}$ ")
    }

    /// 执行一行命令，返回输出（不含提示符；空串 = 无输出）。
    pub fn execute(&mut self, line: &str) -> String {
        let line = line.trim();
        if line.is_empty() {
            return String::new();
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();

        match tokens[0] {
            "pwd" => self.path_string(),
            "ls" => self.ls(tokens.get(1).copied()),
            "cd" => self.cd(tokens.get(1).copied().unwrap_or("/")),
            "mkdir" => match tokens.get(1) {
                Some(name) => self.mkdir(name),
                None => "mkdir: missing operand".to_string(),
            },
            "cat" => match tokens.get(1) {
                Some(name) => self.cat(name),
                None => "cat: missing operand".to_string(),
            },
            "echo" => self.echo(&tokens[1..]),
            other => format!("sh: {other}: command not found"),
        }
    }

    fn path_string(&self) -> String {
        if self.cwd.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.cwd.join("/"))
        }
    }

    /// 解析相对/绝对路径段（支持 `..`、`.`、`/` 前缀），只算路径不做存在性检查
    fn resolve(&self, raw: &str) -> Vec<String> {
        let mut path = if raw.starts_with('/') {
            Vec::new()
        } else {
            self.cwd.clone()
        };
        for seg in raw.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    path.pop();
                }
                s => path.push(s.to_string()),
            }
        }
        path
    }

    fn node_at(&self, path: &[String]) -> Option<&Node> {
        let mut cur: &Node = &self.root;
        for seg in path {
            match cur {
                Node::Dir(dir) => cur = dir.entries.get(seg)?,
                Node::File(_) => return None,
            }
        }
        Some(cur)
    }

    fn dir_at_mut(&mut self, path: &[String]) -> Option<&mut Dir> {
        let mut cur: &mut Node = &mut self.root;
        for seg in path {
            match cur {
                Node::Dir(dir) => cur = dir.entries.get_mut(seg)?,
                Node::File(_) => return None,
            }
        }
        match cur {
            Node::Dir(dir) => Some(dir),
            Node::File(_) => None,
        }
    }

    fn ls(&self, raw: Option<&str>) -> String {
        let path = match raw {
            Some(p) => self.resolve(p),
            None => self.cwd.clone(),
        };
        match self.node_at(&path) {
            Some(Node::Dir(dir)) => dir
                .entries
                .iter()
                .map(|(name, node)| match node {
                    Node::Dir(_) => format!("{name}/"),
                    Node::File(_) => name.clone(),
                })
                .collect::<Vec<_>>()
                .join("  "),
            Some(Node::File(content)) => content.clone(),
            None => String::new(),
        }
    }

    fn cd(&mut self, raw: &str) -> String {
        let target = self.resolve(raw);
        match self.node_at(&target) {
            Some(Node::Dir(_)) => {
                self.cwd = target;
                String::new()
            }
            Some(Node::File(_)) => format!("cd: {raw}: Not a directory"),
            None => format!("cd: {raw}: No such file or directory"),
        }
    }

    fn mkdir(&mut self, raw: &str) -> String {
        let target = self.resolve(raw);
        let Some((name, parent)) = target.split_last() else {
            return "mkdir: invalid path".to_string();
        };
        match self.dir_at_mut(parent) {
            Some(dir) => {
                if dir.entries.contains_key(name) {
                    format!("mkdir: cannot create directory '{raw}': File exists")
                } else {
                    dir.entries.insert(name.clone(), Node::Dir(Dir::default()));
                    String::new()
                }
            }
            None => format!("mkdir: cannot create directory '{raw}': No such file or directory"),
        }
    }

    fn cat(&self, raw: &str) -> String {
        match self.node_at(&self.resolve(raw)) {
            Some(Node::File(content)) => content.clone(),
            Some(Node::Dir(_)) => format!("cat: {raw}: Is a directory"),
            None => format!("cat: {raw}: No such file or directory"),
        }
    }

    /// 执行层事件投影：向虚拟文件系统的日志文件追加一行（活着的文件——
    /// 内容由 VmManager 的真实生命周期事件驱动，cat 读到的是那一刻的历史）。
    pub fn append_line(&mut self, path: &str, line: &str) {
        let target = self.resolve(path);
        let Some((name, parent)) = target.split_last() else {
            return;
        };
        if let Some(dir) = self.dir_at_mut(parent) {
            let entry = dir
                .entries
                .entry(name.clone())
                .or_insert_with(|| Node::File(String::new()));
            if let Node::File(content) = entry {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(line);
            }
        }
    }

    /// echo <text...> [> file | >> file]
    fn echo(&mut self, args: &[&str]) -> String {
        let mut redirect: Option<(bool, String)> = None; // (append, file)
        let mut text_tokens: Vec<&str> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            match args[i] {
                ">" | ">>" => {
                    match args.get(i + 1) {
                        Some(file) => {
                            redirect = Some((args[i] == ">>", (*file).to_string()));
                            i += 1;
                        }
                        None => return "echo: syntax error near unexpected token `newline'".into(),
                    }
                }
                t => text_tokens.push(t),
            }
            i += 1;
        }
        let text = text_tokens.join(" ");

        match redirect {
            None => text,
            Some((append, file)) => {
                let target = self.resolve(&file);
                let Some((name, parent)) = target.split_last() else {
                    return "echo: invalid path".to_string();
                };
                match self.dir_at_mut(parent) {
                    Some(dir) => {
                        let entry = dir
                            .entries
                            .entry(name.clone())
                            .or_insert_with(|| Node::File(String::new()));
                        match entry {
                            Node::File(content) => {
                                if append {
                                    if !content.is_empty() {
                                        content.push('\n');
                                    }
                                    content.push_str(&text);
                                } else {
                                    *content = text;
                                }
                                String::new()
                            }
                            Node::Dir(_) => format!("echo: {file}: Is a directory"),
                        }
                    }
                    None => format!("echo: {file}: No such file or directory"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_walkthrough() {
        let mut sh = Shell::new();
        assert_eq!(sh.execute("pwd"), "/");
        assert!(sh.execute("ls").contains("README.txt"));
        assert_eq!(sh.execute("mkdir a"), "");
        assert_eq!(sh.execute("cd a"), "");
        assert_eq!(sh.execute("pwd"), "/a");
        assert_eq!(sh.execute("echo hello world > f.txt"), "");
        assert_eq!(sh.execute("cat f.txt"), "hello world");
        assert_eq!(sh.execute("echo more >> f.txt"), "");
        assert_eq!(sh.execute("cat f.txt"), "hello world\nmore");
        assert_eq!(sh.execute("cd .."), "");
        assert_eq!(sh.execute("pwd"), "/");
        assert_eq!(sh.execute("ls"), "README.txt  a/  home/  var/");
        assert!(sh.execute("cat nope").contains("No such file"));
        sh.append_line("/var/log/vm.log", "CREATE");
        sh.append_line("/var/log/vm.log", "STOPPED");
        assert_eq!(sh.execute("cat /var/log/vm.log"), "CREATE\nSTOPPED");
        assert!(sh.execute("nonsense").contains("command not found"));
    }
}
