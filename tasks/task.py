#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["typer>=0.15"]
# ///

import base64
import os
import re
import shlex
import subprocess
from pathlib import Path
from typing import Annotated

import typer

app = typer.Typer(help="Unigraph task runner.", invoke_without_command=True)
build_app = typer.Typer(help="Build artifacts.", invoke_without_command=True)
check_app = typer.Typer(
    help="Run linters and format checks.", invoke_without_command=True
)
fmt_app = typer.Typer(help="Run formatters.", invoke_without_command=True)
test_app = typer.Typer(help="Run tests.", invoke_without_command=True)

app.add_typer(build_app, name="build")
app.add_typer(check_app, name="check")
app.add_typer(fmt_app, name="fmt")
app.add_typer(test_app, name="test")

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "node_modules" / ".bin"


@app.callback()
def main(ctx: typer.Context) -> None:
    """Unigraph task runner."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@check_app.callback()
def check_main(ctx: typer.Context) -> None:
    """Run linters and format checks."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@build_app.callback()
def build_main(ctx: typer.Context) -> None:
    """Build artifacts."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@fmt_app.callback()
def fmt_main(ctx: typer.Context) -> None:
    """Run formatters."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


def run(args: list[str], env: dict[str, str] | None = None) -> None:
    cmd = Path(args[0])
    try:
        label = str(cmd.relative_to(ROOT))
    except ValueError:
        label = args[0]
    display = [label, *args[1:]]
    print(f"\033[90m$ {shlex.join(display)}\033[0m", flush=True)
    full_env = {**os.environ, **env} if env else None
    result = subprocess.run(args, cwd=ROOT, env=full_env)
    if result.returncode != 0:
        raise typer.Exit(result.returncode)


OUT_DIR = ROOT / ".build"
WASM_OUT_DIR = OUT_DIR / "wasm"
WASM_TARGET_DIR = Path("/tmp/unigraph-target")


@build_app.command("wasm")
def build_wasm() -> None:
    """Build the WASM package and inline it as base64 TypeScript."""
    WASM_OUT_DIR.mkdir(parents=True, exist_ok=True)

    run(
        [
            "cargo",
            "build",
            "--package",
            "unigraph_wasm",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ]
    )

    wasm_path = (
        WASM_TARGET_DIR / "wasm32-unknown-unknown" / "release" / "unigraph_wasm.wasm"
    )
    run(
        [
            "wasm-bindgen",
            "--target",
            "web",
            str(wasm_path),
            "--out-dir",
            str(WASM_OUT_DIR),
        ]
    )

    # Encode wasm to base64 and create a TS module that inlines it
    wasm_artifact = WASM_OUT_DIR / "unigraph_wasm_bg.wasm"
    wasm_b64 = base64.b64encode(wasm_artifact.read_bytes()).decode()
    wasm_ts = WASM_OUT_DIR / "unigraph_wasm_base64.ts"
    wasm_ts.write_text(f"export default `\n{wasm_b64}\n`;\n")


@build_app.command("js")
def build_js() -> None:
    """Build the JS bundle."""
    run([str(BIN / "rolldown"), "-c", "rolldown.config.ts"])


@build_app.command("haste")
def build_haste() -> None:
    """Build WASM + JS bundle with haste post-processing."""
    build_wasm()
    build_js()

    umd_build = OUT_DIR / "unigraph-explorer-intern.js"
    haste_build = OUT_DIR / "unigraph-explorer-umd-haste-build.js"
    content = umd_build.read_text()

    # cx() is a haste built-in; rename to clsx to avoid conflicts
    content = re.sub(r"\b(cx)\(", "clsx(", content)
    # jsx-runtime at Meta comes directly from react
    content = content.replace("react/jsx-runtime", "react")
    # import.meta.url causes syntax errors in non-module contexts
    content = content.replace("import.meta.url", '"import.meta.url not supported"')

    haste_build.write_text(content)


@build_app.command("react-router")
def build_react_router() -> None:
    """Build the React Router SPA for production."""
    run([str(BIN / "react-router"), "build"])


@build_app.command("all")
def build_all() -> None:
    """Build everything."""
    build_haste()


@app.command(
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True}
)
def serve(ctx: typer.Context) -> None:
    """Start Unigraph server. All flags are forwarded to `unigraph_cli serve`."""
    run(["cargo", "run", "-p", "unigraph_cli", "--", "serve", *ctx.args])


@app.command(
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True}
)
def cli(
    ctx: typer.Context,
    release: Annotated[
        bool, typer.Option("-r", "--release", help="Build in release mode.")
    ] = False,
) -> None:
    """Run unigraph_cli with arbitrary arguments. All args are forwarded."""
    cargo = ["cargo", "run", "-p", "unigraph_cli"]
    if release:
        cargo.append("--release")
    run([*cargo, "--", *ctx.args])


@app.command()
def dev() -> None:
    """Start dev server (Tailwind watch + Rolldown watch)."""
    tailwind = subprocess.Popen(
        [
            str(BIN / "tailwindcss"),
            "-i",
            "./input.css",
            "-o",
            "../.build/output.css",
            "--watch",
            "--cwd",
            "u-fe",
        ],
        cwd=ROOT,
    )
    rolldown = subprocess.Popen(
        [str(BIN / "rolldown"), "-c", "./rolldown.config.ts", "-w"],
        cwd=ROOT,
    )
    try:
        tailwind.wait()
        rolldown.wait()
    except KeyboardInterrupt:
        tailwind.terminate()
        rolldown.terminate()


@check_app.command("js")
def check_js() -> None:
    """Lint, type-check, and check formatting for JavaScript/TypeScript."""
    run([str(BIN / "oxlint"), "-c", ".oxlintrc.json", "u-fe/"])
    run([str(BIN / "oxfmt"), "--check", "u-fe/"])
    run([str(BIN / "tsc"), "--noEmit", "--project", "tsconfig.json"])


@check_app.command("rust")
def check_rust() -> None:
    """Lint and check formatting for Rust."""
    run(["cargo", "fmt", "--check"])
    run(["cargo", "clippy", "--all-targets"])


@check_app.command("all")
def check_all() -> None:
    """Run all checks."""
    check_js()
    check_rust()


@fmt_app.command("js")
def fmt_js() -> None:
    """Format JavaScript/TypeScript with oxfmt."""
    run([str(BIN / "oxfmt"), "u-fe/"])


@fmt_app.command("rust")
def fmt_rust() -> None:
    """Format Rust with cargo fmt."""
    run(["cargo", "fmt"])


@fmt_app.command("all")
def fmt_all() -> None:
    """Run all formatters."""
    fmt_js()
    fmt_rust()


@test_app.callback()
def test_main(ctx: typer.Context) -> None:
    """Run tests."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@test_app.command("js")
def test_js() -> None:
    """Run JavaScript/TypeScript tests with vitest."""
    run([str(BIN / "vitest"), "run"])


@test_app.command("rust")
def test_rust(
    update: Annotated[
        bool, typer.Option("-u", "--update", help="Update k9 snapshots.")
    ] = False,
) -> None:
    """Run Rust tests with cargo-nextest."""
    env = {"K9_UPDATE_SNAPSHOTS": "1"} if update else None
    run(["cargo", "nextest", "run"], env=env)


@test_app.command("all")
def test_all(
    update: Annotated[
        bool, typer.Option("-u", "--update", help="Update k9 snapshots.")
    ] = False,
) -> None:
    """Run all tests."""
    test_js()
    test_rust(update=update)


if __name__ == "__main__":
    app()
