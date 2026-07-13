PHONY_TARGETS += guest shim runtime cli cli\:release skillbox-image build\:apps

BUILD_PROFILE ?= release

guest:
	@bash $(SCRIPT_DIR)/build/build-guest.sh --profile $(BUILD_PROFILE)

shim:
	@bash $(SCRIPT_DIR)/build/build-shim.sh --profile $(BUILD_PROFILE)

runtime: BUILD_PROFILE := release
runtime: guest shim
	@bash $(SCRIPT_DIR)/build/build-runtime.sh --profile $(BUILD_PROFILE)

runtime\:debug: BUILD_PROFILE := debug
runtime\:debug: guest shim
	@bash $(SCRIPT_DIR)/build/build-runtime.sh --profile $(BUILD_PROFILE)

cli: runtime\:debug
	@echo "🔨 Building boxlite CLI..."
	@bash $(SCRIPT_DIR)/build/build-cli.sh --profile debug
	@echo "✅ CLI built: ./target/debug/boxlite"

cli\:release: runtime
	@echo "🔨 Building boxlite CLI (release)..."
	@bash $(SCRIPT_DIR)/build/build-cli.sh --profile release
	@echo "✅ CLI built: ./target/release/boxlite"

# Build the apps/ workspace (api, dashboard, runner, proxy, libs…) via the
# repo's own blessed script (nx run-many --target=build --all). The webpack
# build runs tsc, so this is the compile gate for apps/ changes.
build\:apps: _ensure-apps-deps
	@echo "🔨 Building apps workspace..."
	@cd apps && yarn build
	@echo "✅ apps workspace built → dist/apps"

# Build SkillBox container image (all-in-one AI CLI with noVNC)
# Usage: make skillbox-image [APT_SOURCE=mirrors.aliyun.com]
skillbox-image:
	@echo "🐳 Building SkillBox container image..."
	@docker build $(if $(APT_SOURCE),--build-arg APT_SOURCE=$(APT_SOURCE)) -t boxlite-skillbox:latest src/boxlite/resources/images/skillbox/
	@echo "✅ SkillBox image built: boxlite-skillbox:latest"
