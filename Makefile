# Everything the gate runs, and installing the plugin into Omarchy.
.PHONY: check install reload uninstall

PLUGIN_ID := io.github.itsgg.thodu
DEST := $(HOME)/.config/omarchy/plugins/$(PLUGIN_ID)
# The plugin alone: the Rust workspace, its build and the tests stay here.
PLUGIN_FILES := manifest.json Thodu.qml components

check:
	@tools/gate

install:
	@mkdir -p "$(DEST)"
	@rsync -a --checksum --delete $(PLUGIN_FILES) "$(DEST)/"
	@echo "installed; enable with: omarchy plugin enable $(PLUGIN_ID)"

reload:
	@$(MAKE) --no-print-directory install
	@rm -rf "$(HOME)/.cache/quickshell/qmlcache"
	@omarchy restart shell

uninstall:
	@omarchy plugin disable $(PLUGIN_ID) 2>/dev/null || true
	@rm -rf "$(DEST)"
	@echo "removed"
