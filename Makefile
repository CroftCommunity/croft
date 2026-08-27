# croft — the entry points.
#
# Every target below is meant to be safe to run repeatedly. If one of them is
# ever scary to run, that is a defect in the target, not a reason to be careful.

SHELL := /usr/bin/env bash
ENV   := env

.PHONY: help bootstrap verify record-checksums emulator emulator-ui emulator-nuke \
        install run logcat crash screenshot gate bindings ffi-android \
        android-local-properties clean

help: ## show this help
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

## ---- environment ----------------------------------------------------------

bootstrap: ## zero -> working toolchain (idempotent; safe to re-run)
	@$(ENV)/bootstrap.sh

verify: ## refuse if the installed toolchain differs from env/toolchain.yml
	@$(ENV)/verify.sh

android-local-properties: ## (re)write android/local.properties from the SDK this machine has
	@$(ENV)/android-local-properties.sh

record-checksums: ## fetch pinned artifacts and record their real sha256 into toolchain.yml
	@$(ENV)/record-checksums.sh

## ---- emulator (a definition, not a pet) -----------------------------------

emulator: ## boot the AVD headless (agent-driven loop)
	@$(ENV)/emulator.sh start --headless

emulator-ui: ## boot the AVD with a window (human-driven)
	@$(ENV)/emulator.sh start --window

emulator-nuke: ## destroy the AVD; recreating it is meant to be a non-event
	@$(ENV)/emulator.sh nuke

## ---- the inner loop -------------------------------------------------------

install: ## build the debug APK and install it
	@cd android && ./gradlew assembleDebug && adb install -r app/build/outputs/apk/debug/app-debug.apk

run: install ## install, then launch
	@adb shell monkey -p ing.croft.call -c android.intent.category.LAUNCHER 1

crash: ## the crash buffer only — 20-40 lines, not thousands
	@adb logcat -d -b crash

logcat: ## live log, filtered to this app and fatals
	@adb logcat AndroidRuntime:E ing.croft.call:V '*:S'

screenshot: ## capture the screen from a HEADLESS emulator (no window, no human)
	@adb exec-out screencap -p > /tmp/croft-screen.png && echo "wrote /tmp/croft-screen.png"

## ---- the ffi surface -------------------------------------------------------

bindings: ## build the cdylib, generate Kotlin, run the JVM wiring test
	@$(ENV)/gen-kotlin-bindings.sh

ffi-android: ## cross-compile croft-ffi for arm64 and load it on the emulator
	@$(ENV)/build-croft-ffi-android.sh

## ---- gate -----------------------------------------------------------------

gate: verify ## everything CI runs, in the same order
	@cargo test --workspace
	@$(ENV)/gen-kotlin-bindings.sh
	@$(ENV)/android-local-properties.sh
	@cd android && ./gradlew testDebugUnitTest

clean:
	@cargo clean 2>/dev/null || true
	@cd android && ./gradlew clean 2>/dev/null || true
