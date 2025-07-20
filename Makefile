# Makefile for ngx_vts_rust module

# Default nginx version
NGX_VERSION ?= 1.24.0
NGX_DEBUG ?= 0

# Build configuration
CARGO_FLAGS = --release
ifeq ($(NGX_DEBUG), 1)
    CARGO_FLAGS = 
    export NGX_DEBUG=1
endif

# Export nginx version
export NGX_VERSION

.PHONY: all build clean install test help

# Default target
all: build

# Build the module
build:
	@echo "Building ngx_vts_rust module..."
	@echo "Nginx version: $(NGX_VERSION)"
	@echo "Debug mode: $(NGX_DEBUG)"
	cargo build $(CARGO_FLAGS)
	@echo "Module built successfully!"
	@echo "Location: target/$(if $(findstring --release,$(CARGO_FLAGS)),release,debug)/libngx_vts_rust.so"

# Clean build artifacts
clean:
	cargo clean
	@echo "Build artifacts cleaned."

# Install module (requires sudo for system nginx)
install: build
	@if [ -z "$(NGX_MODULES_PATH)" ]; then \
		echo "Error: NGX_MODULES_PATH not set"; \
		echo "Usage: make install NGX_MODULES_PATH=/path/to/nginx/modules"; \
		exit 1; \
	fi
	cp target/$(if $(findstring --release,$(CARGO_FLAGS)),release,debug)/libngx_vts_rust.so $(NGX_MODULES_PATH)/
	@echo "Module installed to $(NGX_MODULES_PATH)/"

# Run tests
test:
	cargo test
	@echo "Tests completed."

# Development build with debug symbols
debug:
	$(MAKE) build NGX_DEBUG=1

# Check code formatting and linting
check:
	cargo fmt --check
	cargo clippy -- -D warnings
	@echo "Code check completed."

# Format code
fmt:
	cargo fmt
	@echo "Code formatted."

# Generate documentation
docs:
	cargo doc --no-deps --open
	@echo "Documentation generated."

# Show help
help:
	@echo "Available targets:"
	@echo "  build     - Build the module (release mode)"
	@echo "  debug     - Build the module (debug mode)"
	@echo "  clean     - Clean build artifacts"
	@echo "  install   - Install module (requires NGX_MODULES_PATH)"
	@echo "  test      - Run tests"
	@echo "  check     - Check code formatting and linting"
	@echo "  fmt       - Format code"
	@echo "  docs      - Generate documentation"
	@echo "  help      - Show this help"
	@echo ""
	@echo "Environment variables:"
	@echo "  NGX_VERSION       - Nginx version (default: 1.24.0)"
	@echo "  NGX_DEBUG         - Enable debug mode (0/1, default: 0)"
	@echo "  NGX_MODULES_PATH  - Path for module installation"
	@echo ""
	@echo "Examples:"
	@echo "  make build"
	@echo "  make debug"
	@echo "  make install NGX_MODULES_PATH=/etc/nginx/modules"
	@echo "  make build NGX_VERSION=1.25.0"

# Development workflow
dev: clean debug test
	@echo "Development build and test completed."