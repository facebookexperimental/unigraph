#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["typer>=0.15"]
# ///

import shlex
import subprocess
from pathlib import Path
from typing import Annotated

import typer

app = typer.Typer(help="Unigraph task runner.", invoke_without_command=True)
lint_app = typer.Typer(help="Run linters.", invoke_without_command=True)
fmt_app = typer.Typer(help="Run formatters.", invoke_without_command=True)

app.add_typer(lint_app, name="lint")
app.add_typer(fmt_app, name="fmt")

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "node_modules" / ".bin"


@app.callback()
def main(ctx: typer.Context) -> None:
    """Unigraph task runner."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@lint_app.callback()
def lint_main(ctx: typer.Context) -> None:
    """Run linters."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@fmt_app.callback()
def fmt_main(ctx: typer.Context) -> None:
    """Run formatters."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


def run(args: list[str]) -> None:
    display = [str(Path(args[0]).relative_to(ROOT)), *args[1:]]
    print(f"\033[90m$ {shlex.join(display)}\033[0m", flush=True)
    result = subprocess.run(args, cwd=ROOT)
    if result.returncode != 0:
        raise typer.Exit(result.returncode)


@lint_app.command("js")
def lint_js() -> None:
    """Lint JavaScript/TypeScript with oxlint."""
    run([str(BIN / "oxlint"), "-c", ".oxlintrc.json", "u-fe/"])


@lint_app.command("all")
def lint_all() -> None:
    """Run all linters."""
    lint_js()


@fmt_app.command("js")
def fmt_js(
    check: Annotated[
        bool, typer.Option("--check", help="Check formatting without writing.")
    ] = False,
) -> None:
    """Format JavaScript/TypeScript with oxfmt."""
    args = [str(BIN / "oxfmt")]
    if check:
        args.append("--check")
    args.append("u-fe/")
    run(args)


@fmt_app.command("all")
def fmt_all(
    check: Annotated[
        bool, typer.Option("--check", help="Check formatting without writing.")
    ] = False,
) -> None:
    """Run all formatters."""
    fmt_js(check=check)


if __name__ == "__main__":
    app()
