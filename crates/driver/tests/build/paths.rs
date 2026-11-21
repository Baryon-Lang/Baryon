//! `pathlib.Path` and the directory functions of `os` (decision 0036, parts 1 and 7), built and
//! run. Each expected output is what CPython 3.13 prints for the same program, which is also
//! valid Python but for `len` of `iterdir()`, a list here and an iterator there.

use super::{baryon_build, package};
use std::fs;
use std::process::Command;

/// The parts of a path, `/`, `==`, `str` and f-strings never touch the file system and give
/// what `pathlib` gives: doubled slashes and `.` go, `..` stays, exactly two leading slashes
/// stay, and a name that starts or ends with its only `.` has no suffix. A path that names
/// nothing answers `False`, and reading it raises `OSError`.
#[test]
fn paths_have_the_parts_pathlib_gives() {
    let program = "\
from pathlib import Path


def main():
    for text in [\"runs//out/\", \"./a/../b.tar.gz\", \"\", \"/\", \"//host/x\", \"///x/.bashrc\", \"a.\"]:
        path = Path(text)
        parent = path.parent
        print(path, \"|\", path.name, \"|\", path.stem, \"|\", path.suffix, \"|\", parent)
    base = Path(\"runs\")
    out = base / \"2026\" / Path(\"energy.csv\")
    print(out, base / \"/abs\", \"pre\" / base, out.parent.parent == base, out != base)
    print(f\"{out} has {len(str(out))} characters\")
    try:
        print(out.exists(), base.is_dir(), base.is_file())
        (base / \"missing.txt\").read_text()
    except OSError as error:
        print(error.errno, error.strerror, error.filename)
";
    let directory = package("path_parts", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "runs/out | out | out |  | runs\n\
         a/../b.tar.gz | b.tar.gz | b.tar | .gz | a/..\n\
         . |  |  |  | .\n\
         / |  |  |  | /\n\
         //host/x | x | x |  | //host\n\
         /x/.bashrc | .bashrc | .bashrc |  | /x\n\
         a. | a. | a. |  | .\n\
         runs/2026/energy.csv /abs pre/runs True True\n\
         runs/2026/energy.csv has 20 characters\n\
         False False False\n\
         2 No such file or directory runs/missing.txt\n"
    );
}

/// The parts of a path are new values, so a loop that borrows its paths can keep them: the
/// checker once reported `name = entry.name` as moving the borrowed `entry` (roadmap 1.9.31).
#[test]
fn parts_of_borrowed_paths_are_kept() {
    let program = "\
from pathlib import Path


def names(paths: list[Path]) -> list[str]:
    out: list[str] = []
    for entry in paths:
        out.append(entry.name)
    return out


def main():
    paths = [Path(\"runs/a.csv\"), Path(\"runs/b.npy\")]
    for entry in paths:
        name = entry.name
        parent = entry.parent
        print(name, entry.stem, parent, entry)
    first = paths[0].suffix
    print(names(paths), first)
";
    let directory = package("borrowed_path_parts", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "a.csv a runs runs/a.csv\n\
         b.npy b runs runs/b.npy\n\
         ['a.csv', 'b.npy'] .csv\n"
    );
}

/// A program makes directories, writes and reads a file through a `Path`, renames and removes
/// it, and lists what is there; then each call that cannot work raises the `OSError` Python
/// raises, with the same `errno`, text and file name.
#[test]
fn directories_are_made_listed_and_removed() {
    let program = "\
import os
from pathlib import Path


def main():
    try:
        os.mkdir(\"work\")
        os.makedirs(Path(\"work\") / \"a\" / \"b\")
        os.makedirs(\"work/a/b\", exist_ok=True)
        note = Path(\"work/a\") / \"note.txt\"
        print(note.write_text(\"énergie\\n\"), note.read_text(), end=\"\")
        print(note.exists(), note.is_file(), note.is_dir(), note.parent.is_dir())
        os.rename(note, \"work/moved.txt\")
        print(note.exists(), sorted(os.listdir(\"work\")))
        names: list[str] = []
        for entry in Path(\"work\").iterdir():
            names.append(entry.name + entry.suffix)
        print(sorted(names), len(Path(\"work/a/b\").iterdir()))
        os.remove(\"work/moved.txt\")
        print(os.listdir(\"work\"), \"work\" in os.listdir())
        print(os.getcwd())
    except OSError as error:
        print(\"unexpected\", error.strerror)
    try:
        os.mkdir(\"work\")
    except OSError as error:
        print(error.errno, error.strerror, error.filename)
    try:
        os.makedirs(\"work/a\")
    except OSError as error:
        print(error.errno, error.strerror, error.filename)
    try:
        os.mkdir(\"none/below\")
    except OSError as error:
        print(error.errno, error.strerror, error.filename)
    try:
        os.remove(\"work/none.txt\")
    except OSError as error:
        print(error.errno, error.strerror, error.filename)
    try:
        os.rename(\"work/none.txt\", \"work/other.txt\")
    except OSError as error:
        print(error.errno, error.strerror, error.filename)
    try:
        os.listdir(Path(\"work\") / \"none\")
    except OSError as error:
        print(error.errno, error.strerror, error.filename)
";
    let directory = package("directories", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    let here = fs::canonicalize(&*directory).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!(
            "8 énergie\n\
             True True False True\n\
             False ['a', 'moved.txt']\n\
             ['a', 'moved.txt.txt'] 0\n\
             ['a'] True\n\
             {}\n\
             17 File exists work\n\
             17 File exists work/a\n\
             2 No such file or directory none/below\n\
             2 No such file or directory work/none.txt\n\
             2 No such file or directory work/none.txt\n\
             2 No such file or directory work/none\n",
            here.display()
        )
    );
    assert!(directory.join("work/a/b").is_dir());
    assert!(!directory.join("work/moved.txt").exists());
}
