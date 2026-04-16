mod error;
mod lexer;
mod ast;
mod parser;
mod value;
mod interpreter;
mod stdlib;
mod bot;
mod fmt;
mod lsp;

use std::sync::Arc;
use clap::{Parser as ClapParser, Subcommand};
use colored::Colorize;

use crate::error::GravResult;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::bot::BotRunner;
use crate::value::BotOutput;

// ─────────────────────────────────────────────────────────────────────────────
// CLI
// ─────────────────────────────────────────────────────────────────────────────

#[derive(ClapParser)]
#[command(
    name    = "gravitix",
    version = "0.1.0",
    about   = "Gravitix — fast bot scripting language for Vortex",
    long_about = r#"
  ██████╗ ██████╗  █████╗ ██╗   ██╗██╗████████╗██╗██╗  ██╗
 ██╔════╝ ██╔══██╗██╔══██╗██║   ██║██║╚══██╔══╝██║╚██╗██╔╝
 ██║  ███╗██████╔╝███████║██║   ██║██║   ██║   ██║ ╚███╔╝
 ██║   ██║██╔══██╗██╔══██║╚██╗ ██╔╝██║   ██║   ██║ ██╔██╗
 ╚██████╔╝██║  ██║██║  ██║ ╚████╔╝ ██║   ██║   ██║██╔╝ ██╗
  ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝  ╚═══╝  ╚═╝   ╚═╝   ╚═╝╚═╝  ╚═╝
  Fast scripting language for Vortex bots"#
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a Gravitix bot script connected to Vortex
    Run {
        /// Path to the .grav script
        file: String,

        /// Vortex bot token (overrides BOT_TOKEN env var)
        #[arg(short, long)]
        token: Option<String>,

        /// Vortex server URL (default: http://localhost:8000 or VORTEX_URL env)
        #[arg(short, long)]
        url: Option<String>,
    },

    /// Check syntax of a .grav script without running it
    Check {
        file: String,
    },

    /// Pretty-print (format) a .grav script  [writes to stdout; use > file to overwrite]
    Fmt {
        file: String,
        /// Write result back to the file instead of stdout
        #[arg(short, long)]
        write: bool,
    },

    /// Start an interactive REPL (expressions, statements, multi-line)
    Repl {
        /// Optional .grav script to load before entering REPL
        #[arg(required = false)]
        file: Option<String>,
    },

    /// Run all `test "…" { … }` blocks in a script
    Test {
        file: String,
    },

    /// Start a Language Server Protocol server (stdin/stdout, for IDE integration)
    Lsp,

    /// Generate documentation from doc comments in a .grav script
    Doc {
        file: String,
    },

    /// Install a Gravitix package (stub)
    Install {
        name: String,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        // Fall back to basic display for top-level errors (no source available here)
        let diag = e.to_diagnostic("<unknown>", "");
        eprint!("{}", diag.display("<unknown>"));
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> GravResult<()> {
    match cli.command {
        Command::Run { file, token, url } => {
            let src = std::fs::read_to_string(&file)?;
            let prog = compile(&src, &file)?;
            let token = token
                .or_else(|| std::env::var("BOT_TOKEN").ok())
                .ok_or_else(|| crate::error::GravError::Runtime(
                    "Bot token required. Use --token or set BOT_TOKEN env var.".into()
                ))?;
            let url = url
                .or_else(|| std::env::var("VORTEX_URL").ok())
                .unwrap_or_else(|| "http://localhost:8000".into());
            println!("{}", "[gravitix] Connecting to Vortex…".green());
            let runner = BotRunner::new(url, token, Arc::new(prog));
            runner.run().await?;
        }

        Command::Check { file } => {
            let src = std::fs::read_to_string(&file)?;
            match compile(&src, &file) {
                Ok(prog) => {
                    println!("{} {} ({} items)",
                        "ok:".green().bold(), file,
                        prog.items.len());
                }
                Err(_) => {
                    // Diagnostic already printed by compile()
                    std::process::exit(1);
                }
            }
        }

        Command::Fmt { file, write } => {
            let src  = std::fs::read_to_string(&file)?;
            let prog = compile(&src, &file)?;
            let formatted = fmt::format_program(&prog);
            if write {
                std::fs::write(&file, &formatted)?;
                println!("{} {file}", "formatted:".green().bold());
            } else {
                print!("{formatted}");
            }
        }

        Command::Repl { file } => {
            if let Some(ref f) = file {
                // Load the script first, then enter REPL
                let src = std::fs::read_to_string(f)?;
                let _ = compile(&src, f)?;
                println!("{} loaded {f}", "[gravitix]".green());
            }
            run_repl().await?;
        }

        Command::Test { file } => {
            run_tests(&file).await?;
        }

        Command::Lsp => {
            lsp::run_lsp();
        }

        Command::Doc { file } => {
            let src = std::fs::read_to_string(&file)?;
            let prog = compile(&src, &file)?;
            println!("# Documentation for {file}\n");
            for item in &prog.items {
                match item {
                    crate::ast::Item::FnDef(fd) => {
                        if let Some(ref doc) = fd.doc {
                            println!("## fn {}\n\n{doc}\n", fd.name);
                        }
                    }
                    crate::ast::Item::Handler(h) => {
                        if let Some(ref doc) = h.doc {
                            let trigger = fmt::format_trigger_pub(&h.trigger);
                            println!("## on {trigger}\n\n{doc}\n");
                        }
                    }
                    crate::ast::Item::FlowDef(fd) => {
                        if let Some(ref doc) = fd.doc {
                            println!("## flow {}\n\n{doc}\n", fd.name);
                        }
                    }
                    _ => {}
                }
            }
        }

        Command::Install { name } => {
            println!("{} Installing package '{name}'…", "[gravitix]".green());
            println!("{} Package manager is a stub — not yet implemented.", "warn:".yellow().bold());
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Compile: lex + parse
// ─────────────────────────────────────────────────────────────────────────────

fn compile(src: &str, filename: &str) -> GravResult<crate::ast::Program> {
    let tokens = Lexer::new(src).tokenize().map_err(|e| {
        let diag = e.to_diagnostic(filename, src);
        eprint!("{}", diag.display(filename));
        e
    })?;
    let prog = Parser::new(tokens).parse().map_err(|e| {
        let diag = e.to_diagnostic(filename, src);
        eprint!("{}", diag.display(filename));
        e
    })?;
    Ok(prog)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test runner
// ─────────────────────────────────────────────────────────────────────────────

async fn run_tests(file: &str) -> GravResult<()> {
    let src  = std::fs::read_to_string(file)?;
    let prog = compile(&src, file)?;

    let interp = interpreter::Interpreter::new(String::new(), String::new());
    interp.load(&prog).await?;

    let results = interp.run_tests(&prog).await;

    if results.is_empty() {
        println!("{} no test blocks found in {file}", "warn:".yellow().bold());
        return Ok(());
    }

    let mut passed = 0usize;
    let mut failed = 0usize;

    for (name, outcome) in &results {
        match outcome {
            Ok(()) => {
                println!("  {} {name}", "✓".green().bold());
                passed += 1;
            }
            Err(msg) => {
                println!("  {} {name}", "✗".red().bold());
                println!("    {}", msg.red());
                failed += 1;
            }
        }
    }

    println!();
    let total = passed + failed;
    if failed == 0 {
        println!("{} {passed}/{total} tests passed", "ok:".green().bold());
    } else {
        println!("{} {failed}/{total} tests failed", "FAILED:".red().bold());
        std::process::exit(1);
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// REPL  (improved: statements, multi-line input, in-memory history)
// ─────────────────────────────────────────────────────────────────────────────

async fn run_repl() -> GravResult<()> {
    use std::io::{self, Write};

    println!("{}", "Gravitix REPL  —  type statements/expressions, :q to quit, :h for history".cyan());
    println!("{}", "  Multi-line: end a line with '{' or ',' to continue on next line".dimmed());

    let interp = interpreter::Interpreter::new(String::new(), String::new());
    let mut env = interpreter::Env::new();
    let mut history: Vec<String> = Vec::new();

    let stdin = io::stdin();
    loop {
        print!("{} ", ">>".bold().purple());
        io::stdout().flush().ok();

        // Collect potentially multi-line input
        let src = read_repl_input(&stdin)?;
        let src = src.trim();
        if src.is_empty() { continue; }

        // Built-in REPL commands
        if src == ":q" || src == ":quit" { println!("bye!"); break; }
        if src == ":h" || src == ":history" {
            for (i, h) in history.iter().enumerate() {
                println!("  {i}: {h}");
            }
            continue;
        }

        history.push(src.to_string());

        // Try as statement first, then as expression
        match eval_repl_line(src, &interp, &mut env).await {
            Ok(Some(v)) if !matches!(v, value::Value::Null) => {
                println!("= {}", v.to_string().yellow());
            }
            Ok(_) => {}
            Err(e) => {
                let diag = e.to_diagnostic("<repl>", src);
                eprint!("{}", diag.display("<repl>"));
                // Include call stack if non-empty
                let tb = interp.format_traceback().await;
                if !tb.is_empty() { eprintln!("{}", tb.dimmed()); }
            }
        }
    }
    Ok(())
}

fn read_repl_input(stdin: &std::io::Stdin) -> GravResult<String> {
    use std::io::{BufRead, Write};
    let mut buf = String::new();
    let mut depth: i32 = 0;

    loop {
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            break;
        }
        // Track brace depth for multi-line blocks
        for ch in line.chars() {
            match ch { '{' => depth += 1, '}' => depth -= 1, _ => {} }
        }
        buf.push_str(&line);
        // If braces are balanced and last non-whitespace isn't ',', we're done
        if depth <= 0 {
            let trimmed = buf.trim_end();
            if !trimmed.ends_with(',') && !trimmed.ends_with('\\') {
                break;
            }
        }
        // Prompt for continuation
        print!("{} ", "..".bold().dimmed());
        std::io::stdout().flush().ok();
    }
    Ok(buf)
}

async fn eval_repl_line(
    src:   &str,
    interp: &interpreter::Interpreter,
    env:    &mut interpreter::Env,
) -> GravResult<Option<value::Value>> {
    let tokens = Lexer::new(src).tokenize()?;

    // 1. Try as a top-level program item (fn def, handler, let stmt, etc.)
    {
        let p = Parser::new(tokens.clone());
        if let Ok(prog) = p.parse() {
            // Load any fn/flow/state defs, then exec stmts
            interp.load(&prog).await?;
            // Execute bare Stmt items
            let mut outputs: Vec<BotOutput> = Vec::new();
            let mut last = value::Value::Null;
            for item in &prog.items {
                if let crate::ast::Item::Stmt(stmt) = item {
                    match interp.exec_stmt_pub(stmt, env, None, &mut outputs).await {
                        Ok(()) => {}
                        Err(e) => return Err(e),
                    }
                }
                if let crate::ast::Item::FnDef(fd) = item {
                    last = value::Value::make_str(format!("<fn {}>", fd.name));
                }
            }
            return Ok(Some(last));
        }
    }

    // 2. Fall back: parse as single expression
    let mut p = Parser::new(tokens);
    let expr = p.parse_expr_pub()?;
    let v = interp.eval_expr(&expr, env, None).await?;
    Ok(Some(v))
}
