//! In-memory virtual file system (MemFS).
//!
//! NoorOS has no disk driver, so the whole file system lives in RAM as a tree
//! of [`Inode`]s. Paths are resolved from the root; a leading `/` is optional
//! and `..` is rejected so a path can never escape the tree. Nothing survives a
//! reset, which is stated in the seed files and in the documentation.
//!
//! All operations take one spin mutex. Only the desktop task (terminal and
//! monitor) uses the file system, and interrupt handlers never do, so holding
//! the lock across an allocation is safe on this single-CPU kernel.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use spin::Mutex;

use crate::drivers::timer;
use crate::memory;
use crate::tasking;
use crate::util;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileType {
    RegularFile,
    Directory,
}

/// One entry returned by [`list`] or [`stat`].
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub inode: u64,
    pub name: String,
    pub file_type: FileType,
    pub size: u64,
}

/// A file-system node.
///
/// Directories own their children, so deleting a directory removes the whole
/// subtree. Files own their bytes in `data`.
pub struct Inode {
    pub inode: u64,
    pub name: String,
    pub file_type: FileType,
    data: Vec<u8>,
    children: Vec<Inode>,
}

impl Inode {
    fn directory(inode: u64, name: String) -> Self {
        Self {
            inode,
            name,
            file_type: FileType::Directory,
            data: Vec::new(),
            children: Vec::new(),
        }
    }

    fn file(inode: u64, name: String, data: &[u8]) -> Self {
        Self {
            inode,
            name,
            file_type: FileType::RegularFile,
            data: data.to_vec(),
            children: Vec::new(),
        }
    }

    pub fn size(&self) -> u64 {
        match self.file_type {
            FileType::RegularFile => self.data.len() as u64,
            FileType::Directory => 0,
        }
    }

    fn entry(&self) -> DirEntry {
        DirEntry {
            inode: self.inode,
            name: self.name.clone(),
            file_type: self.file_type,
            size: self.size(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FsError {
    NotMounted,
    NotFound,
    NotADirectory,
    IsADirectory,
    AlreadyExists,
    InvalidPath,
    Protected,
}

impl FsError {
    pub fn message(self) -> &'static str {
        match self {
            FsError::NotMounted => "the file system is not mounted",
            FsError::NotFound => "no such file or directory",
            FsError::NotADirectory => "not a directory",
            FsError::IsADirectory => "is a directory",
            FsError::AlreadyExists => "already exists",
            FsError::InvalidPath => "invalid path",
            FsError::Protected => "the root directory cannot be removed",
        }
    }
}

#[derive(Clone, Copy)]
pub struct FsStats {
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
}

static ROOT: Mutex<Option<Inode>> = Mutex::new(None);
static NEXT_INODE: AtomicU64 = AtomicU64::new(1);

const README: &str = "\
Welcome to NoorOS.

This file lives in MemFS, an in-memory virtual file
system. Nothing is stored on a disk: every file is
created from RAM at boot and disappears on reset.

Try these commands in Noor Terminal (F2):
  ls /
  cat /etc/os-release
  echo \"hello from MemFS\" > /notes.txt
  cat /notes.txt
  rm /notes.txt
";

const OS_RELEASE: &str = "\
NAME=\"NoorOS\"
VERSION=\"0.1.0\"
ID=nooros
PRETTY_NAME=\"NoorOS 0.1.0\"
ANSI_COLOR=\"1;33\"
";

const MOTD: &str = "\
A Light Beyond Computing.
NoorOS 0.1.0 - Noor Kernel
";

const WELCOME: &str = "\
Welcome, noor.

The MemFS tree supports nested directories.
This file is /home/noor/welcome.txt.
";

/// Creates the root tree and pre-populates the seed files.
///
/// Must run after `memory::init_map` and `tasking::init`, because `/sys/info`
/// is generated from their live values.
pub fn init() -> Result<(), FsError> {
    let mut guard = ROOT.lock();
    if guard.is_some() {
        return Ok(());
    }

    let mut root = Inode::directory(0, String::from("/"));
    add_file(&mut root, "readme.txt", README.as_bytes())?;

    let etc = add_dir(&mut root, "etc")?;
    add_file(etc, "os-release", OS_RELEASE.as_bytes())?;
    add_file(etc, "motd", MOTD.as_bytes())?;

    let sys = add_dir(&mut root, "sys")?;
    add_file(sys, "info", sys_info().as_bytes())?;

    let home = add_dir(&mut root, "home")?;
    let noor = add_dir(home, "noor")?;
    add_file(noor, "welcome.txt", WELCOME.as_bytes())?;

    *guard = Some(root);
    Ok(())
}

/// Creates, writes, reads and deletes a scratch file to prove the tree works.
pub fn self_test() -> Result<(), &'static str> {
    let path = "/.memfs-self-test";
    create_file(path).map_err(|_| "MemFS self-test: create failed")?;
    write_file(path, b"noor").map_err(|_| "MemFS self-test: write failed")?;
    let data = read_file(path).map_err(|_| "MemFS self-test: read failed")?;
    if data != b"noor" {
        return Err("MemFS self-test: data mismatch");
    }
    delete(path).map_err(|_| "MemFS self-test: delete failed")?;
    if exists(path) {
        return Err("MemFS self-test: delete left the file behind");
    }
    Ok(())
}

pub fn create_file(path: &str) -> Result<u64, FsError> {
    let parts = components(path)?;
    let mut guard = ROOT.lock();
    let root = guard.as_mut().ok_or(FsError::NotMounted)?;
    let (parent, name) = parent_mut(root, &parts)?;
    if parent.children.iter().any(|child| child.name == name) {
        return Err(FsError::AlreadyExists);
    }
    let inode = next_inode();
    parent
        .children
        .push(Inode::file(inode, name.to_string(), &[]));
    Ok(inode)
}

pub fn create_dir(path: &str) -> Result<u64, FsError> {
    let parts = components(path)?;
    let mut guard = ROOT.lock();
    let root = guard.as_mut().ok_or(FsError::NotMounted)?;
    let (parent, name) = parent_mut(root, &parts)?;
    if parent.children.iter().any(|child| child.name == name) {
        return Err(FsError::AlreadyExists);
    }
    let inode = next_inode();
    parent
        .children
        .push(Inode::directory(inode, name.to_string()));
    Ok(inode)
}

/// Writes `data` to `path`, creating the file if it does not exist.
pub fn write_file(path: &str, data: &[u8]) -> Result<usize, FsError> {
    let parts = components(path)?;
    let mut guard = ROOT.lock();
    let root = guard.as_mut().ok_or(FsError::NotMounted)?;

    match find_mut(root, &parts) {
        Ok(node) => {
            if node.file_type != FileType::RegularFile {
                return Err(FsError::IsADirectory);
            }
            node.data.clear();
            node.data.extend_from_slice(data);
        }
        Err(FsError::NotFound) => {
            let (parent, name) = parent_mut(root, &parts)?;
            let inode = next_inode();
            parent
                .children
                .push(Inode::file(inode, name.to_string(), data));
        }
        Err(error) => return Err(error),
    }
    Ok(data.len())
}

pub fn read_file(path: &str) -> Result<Vec<u8>, FsError> {
    let parts = components(path)?;
    let guard = ROOT.lock();
    let root = guard.as_ref().ok_or(FsError::NotMounted)?;
    let node = find(root, &parts)?;
    if node.file_type != FileType::RegularFile {
        return Err(FsError::IsADirectory);
    }
    Ok(node.data.clone())
}

/// Deletes a file or a directory (and, for a directory, its whole subtree).
pub fn delete(path: &str) -> Result<(), FsError> {
    let parts = components(path)?;
    if parts.is_empty() {
        return Err(FsError::Protected);
    }
    let mut guard = ROOT.lock();
    let root = guard.as_mut().ok_or(FsError::NotMounted)?;
    let (parent, name) = parent_mut(root, &parts)?;
    let index = parent
        .children
        .iter()
        .position(|child| child.name == name)
        .ok_or(FsError::NotFound)?;
    parent.children.remove(index);
    Ok(())
}

/// Lists a directory. Entries are sorted directories-first, then by name.
pub fn list(path: &str) -> Result<Vec<DirEntry>, FsError> {
    let parts = components(path)?;
    let guard = ROOT.lock();
    let root = guard.as_ref().ok_or(FsError::NotMounted)?;
    let node = find(root, &parts)?;
    if node.file_type != FileType::Directory {
        return Err(FsError::NotADirectory);
    }
    let mut entries: Vec<DirEntry> = node.children.iter().map(Inode::entry).collect();
    entries.sort_by(|a, b| match (a.file_type, b.file_type) {
        (FileType::Directory, FileType::RegularFile) => core::cmp::Ordering::Less,
        (FileType::RegularFile, FileType::Directory) => core::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    Ok(entries)
}

pub fn stat(path: &str) -> Result<DirEntry, FsError> {
    let parts = components(path)?;
    let guard = ROOT.lock();
    let root = guard.as_ref().ok_or(FsError::NotMounted)?;
    Ok(find(root, &parts)?.entry())
}

pub fn exists(path: &str) -> bool {
    stat(path).is_ok()
}

pub fn stats() -> FsStats {
    let guard = ROOT.lock();
    let mut stats = FsStats {
        files: 0,
        directories: 0,
        bytes: 0,
    };
    if let Some(root) = guard.as_ref() {
        for child in &root.children {
            count(child, &mut stats);
        }
    }
    stats
}

fn count(node: &Inode, stats: &mut FsStats) {
    match node.file_type {
        FileType::RegularFile => {
            stats.files += 1;
            stats.bytes += node.size();
        }
        FileType::Directory => {
            stats.directories += 1;
            for child in &node.children {
                count(child, stats);
            }
        }
    }
}

fn next_inode() -> u64 {
    NEXT_INODE.fetch_add(1, Ordering::Relaxed)
}

fn add_dir<'a>(parent: &'a mut Inode, name: &str) -> Result<&'a mut Inode, FsError> {
    if parent.children.iter().any(|child| child.name == name) {
        return Err(FsError::AlreadyExists);
    }
    parent
        .children
        .push(Inode::directory(next_inode(), name.to_string()));
    parent.children.last_mut().ok_or(FsError::NotFound)
}

fn add_file(parent: &mut Inode, name: &str, data: &[u8]) -> Result<(), FsError> {
    if parent.children.iter().any(|child| child.name == name) {
        return Err(FsError::AlreadyExists);
    }
    parent
        .children
        .push(Inode::file(next_inode(), name.to_string(), data));
    Ok(())
}

fn components(path: &str) -> Result<Vec<&str>, FsError> {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(FsError::InvalidPath),
            other => parts.push(other),
        }
    }
    Ok(parts)
}

fn find<'a>(root: &'a Inode, parts: &[&str]) -> Result<&'a Inode, FsError> {
    let mut current = root;
    for part in parts {
        if current.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        current = current
            .children
            .iter()
            .find(|child| child.name == *part)
            .ok_or(FsError::NotFound)?;
    }
    Ok(current)
}

fn find_mut<'a>(root: &'a mut Inode, parts: &[&str]) -> Result<&'a mut Inode, FsError> {
    let mut current = root;
    for part in parts {
        if current.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        current = current
            .children
            .iter_mut()
            .find(|child| child.name == *part)
            .ok_or(FsError::NotFound)?;
    }
    Ok(current)
}

fn parent_mut<'a, 'b>(
    root: &'a mut Inode,
    parts: &'b [&'b str],
) -> Result<(&'a mut Inode, &'b str), FsError> {
    let (name, directories) = parts.split_last().ok_or(FsError::InvalidPath)?;
    let mut current = root;
    for part in directories {
        if current.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        current = current
            .children
            .iter_mut()
            .find(|child| child.name == *part)
            .ok_or(FsError::NotFound)?;
    }
    if current.file_type != FileType::Directory {
        return Err(FsError::NotADirectory);
    }
    Ok((current, name))
}

fn sys_info() -> String {
    let mut text = String::from("kernel=Noor Kernel 0.1.0\narch=x86_64\n");
    if let Some(map) = memory::map() {
        text.push_str(&alloc::format!(
            "memory_usable={}\nmemory_total={}\n",
            util::bytes(map.usable_bytes),
            util::bytes(map.total_bytes)
        ));
    }
    let frames = memory::frame_stats();
    text.push_str(&alloc::format!("frames_free={}\n", frames.free));
    text.push_str(&alloc::format!(
        "tasks={}\nscheduler=round-robin @ {} Hz\n",
        tasking::stats().tasks,
        timer::TICK_HZ
    ));
    text
}
