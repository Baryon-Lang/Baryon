//! The two baseline languages, their compilers, and how a kernel is built.
//!
//! Compilers come from the conventional environment variables, so any installed compiler can be
//! measured without editing code:
//!
//! | Language | Compiler variable | Default    | Extra flags variable |
//! | -------- | ----------------- | ---------- | -------------------- |
//! | C++      | `CXX`             | `clang++`  | `CXXFLAGS`           |
//! | Fortran  | `FC`              | `gfortran` | `FFLAGS`             |
//!
//! Every build uses `-O3` and a fixed language standard, then any extra flags. The flags actually
//! used are recorded in every baseline.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A language every kernel is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    /// C++20.
    Cpp,
    /// Fortran 2018.
    Fortran,
}

impl Language {
    /// Both languages, in report order.
    pub(crate) const ALL: [Self; 2] = [Self::Cpp, Self::Fortran];

    /// The key used in baseline files and build directories.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Cpp => "cpp",
            Self::Fortran => "fortran",
        }
    }

    /// The name shown to people.
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Cpp => "C++",
            Self::Fortran => "Fortran",
        }
    }

    /// The source file extension.
    fn extension(self) -> &'static str {
        match self {
            Self::Cpp => "cpp",
            Self::Fortran => "f90",
        }
    }

    /// The environment variable naming the compiler, and the compiler used when it is unset.
    fn compiler_variable(self) -> (&'static str, &'static str) {
        match self {
            Self::Cpp => ("CXX", "clang++"),
            Self::Fortran => ("FC", "gfortran"),
        }
    }

    /// The environment variable holding extra flags.
    fn flags_variable(self) -> &'static str {
        match self {
            Self::Cpp => "CXXFLAGS",
            Self::Fortran => "FFLAGS",
        }
    }

    /// Flags every build uses, before any extra flags.
    fn base_flags(self) -> &'static [&'static str] {
        match self {
            Self::Cpp => &["-O3", "-std=c++20"],
            Self::Fortran => &["-O3", "-std=f2018"],
        }
    }
}

/// A compiler and the flags it builds every kernel with.
#[derive(Debug)]
pub(crate) struct Compiler {
    /// The language it compiles.
    pub(crate) language: Language,
    /// The command that runs it, such as `clang++` or `gfortran-14`.
    pub(crate) command: String,
    /// Every flag passed on each build, base flags first.
    pub(crate) flags: Vec<String>,
}

impl Compiler {
    /// The compiler for `language`, as chosen by the environment.
    pub(crate) fn from_environment(language: Language) -> Self {
        let (variable, default) = language.compiler_variable();
        let command = env::var(variable).unwrap_or_else(|_| String::from(default));
        let extra = env::var(language.flags_variable()).unwrap_or_default();
        Self::new(language, command, &extra)
    }

    /// A compiler for `language` run as `command`, with whitespace-separated `extra` flags appended
    /// to the base flags.
    fn new(language: Language, command: String, extra: &str) -> Self {
        let flags = language
            .base_flags()
            .iter()
            .map(|flag| String::from(*flag))
            .chain(extra.split_whitespace().map(String::from))
            .collect();
        Self {
            language,
            command,
            flags,
        }
    }

    /// The first line of `<command> --version`, which names the compiler and its version.
    ///
    /// # Errors
    ///
    /// Fails if the compiler cannot be started, which usually means it is not installed.
    pub(crate) fn version(&self) -> Result<String, String> {
        let output = Command::new(&self.command)
            .arg("--version")
            .output()
            .map_err(|error| {
                format!(
                    "cannot run {} compiler `{}`: {error}; install it or set {}",
                    self.language.display_name(),
                    self.command,
                    self.language.compiler_variable().0
                )
            })?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().next().unwrap_or("unknown").trim().to_owned())
    }

    /// Builds kernel `name` from `kernels/<name>/<name>.<extension>` and the shared support file,
    /// writing the executable to `build/<language>/<name>`, and returns the executable's path.
    ///
    /// # Errors
    ///
    /// Fails if the compiler cannot be started or reports an error; the message includes the
    /// compiler's output.
    pub(crate) fn build(
        &self,
        kernels: &Path,
        build: &Path,
        name: &str,
    ) -> Result<PathBuf, String> {
        let output_directory = build.join(self.language.key());
        let modules = output_directory.join(format!("{name}.modules"));
        std::fs::create_dir_all(&modules).map_err(|error| error.to_string())?;
        let executable = output_directory.join(name);
        let source = kernels
            .join(name)
            .join(format!("{name}.{}", self.language.extension()));
        let support = kernels.join("support");

        let mut command = Command::new(&self.command);
        command.args(&self.flags);
        match self.language {
            Language::Cpp => {
                command.arg("-I").arg(&support).arg(&source);
            }
            Language::Fortran => {
                // Module files go to a directory per kernel, so builds never share state.
                command
                    .arg("-J")
                    .arg(&modules)
                    .arg(support.join("support.f90"))
                    .arg(&source);
            }
        }
        command.arg("-o").arg(&executable);

        let output = command
            .output()
            .map_err(|error| format!("cannot run `{}`: {error}", self.command))?;
        if output.status.success() {
            Ok(executable)
        } else {
            Err(format!(
                "building {name} ({}) failed:\n{}",
                self.language.display_name(),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_flags_follow_the_base_flags() {
        let compiler = Compiler::new(
            Language::Fortran,
            String::from("gfortran-14"),
            " -mcpu=native  -flto ",
        );
        assert_eq!(compiler.command, "gfortran-14");
        assert_eq!(
            compiler.flags,
            ["-O3", "-std=f2018", "-mcpu=native", "-flto"]
        );
    }

    #[test]
    fn no_extra_flags_leaves_the_base_flags() {
        let compiler = Compiler::new(Language::Cpp, String::from("clang++"), "");
        assert_eq!(compiler.flags, ["-O3", "-std=c++20"]);
    }
}
