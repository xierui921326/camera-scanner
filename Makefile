.PHONY: help install run dev debug frontend frontend-build \
	check check-rs check-fe clippy build bundle clean clean-rs

PNPM ?= pnpm
CARGO ?= cargo
TAURI := $(PNPM) tauri
SRC_TAURI := src-tauri

# Default: list available targets.
help:
	@echo "camera-scanner Makefile"
	@echo ""
	@echo "Setup"
	@echo "  make install          安装前端依赖 (pnpm install)"
	@echo ""
	@echo "Run / Debug"
	@echo "  make run              启动桌面开发模式 (tauri dev)"
	@echo "  make debug            同 run，带 RUST_LOG=debug"
	@echo "  make frontend         仅启动 Vite 前端 (不启桌面壳)"
	@echo "  make check            前后端静态检查 (tsc + cargo check)"
	@echo "  make check-fe         仅前端 tsc"
	@echo "  make check-rs         仅后端 cargo check"
	@echo "  make clippy           后端 clippy"
	@echo ""
	@echo "Build / Package"
	@echo "  make frontend-build   仅构建前端产物"
	@echo "  make build            打包桌面安装包 (tauri build)"
	@echo "  make bundle           同 build"
	@echo ""
	@echo "Clean"
	@echo "  make clean            清理前端产物与 node_modules"
	@echo "  make clean-rs         清理 Rust target"

# --- Setup ---

install:
	$(PNPM) install

# --- Run / Debug ---

run dev:
	$(TAURI) dev

debug:
	RUST_LOG=debug $(TAURI) dev

frontend:
	$(PNPM) dev

# --- Checks ---

check: check-fe check-rs

check-fe:
	$(PNPM) exec tsc --noEmit

check-rs:
	$(CARGO) check --manifest-path $(SRC_TAURI)/Cargo.toml

clippy:
	$(CARGO) clippy --manifest-path $(SRC_TAURI)/Cargo.toml --all-targets -- -D warnings

# --- Build / Package ---

frontend-build:
	$(PNPM) build

build bundle:
	$(TAURI) build

# --- Clean ---

clean:
	rm -rf dist node_modules

clean-rs:
	$(CARGO) clean --manifest-path $(SRC_TAURI)/Cargo.toml
