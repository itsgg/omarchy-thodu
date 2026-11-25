# Everything the gate runs, and installing the plugin into Omarchy.
.PHONY: check install reload uninstall

PLUGIN_ID := io.github.itsgg.thodu
DEST := $(HOME)/.config/omarchy/plugins/$(PLUGIN_ID)
# The plugin alone: the Rust workspace, its build and the tests stay here.
PLUGIN_FILES := $(shell sed -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$$/d' plugin-files.txt)

check:
	@tools/gate

# Only the listed files and their directories are copied; anything else in
# DEST, or beside them here, is left out or removed.
PLUGIN_DIRS := $(sort $(filter-out ./,$(dir $(PLUGIN_FILES))))

install:
	@mkdir -p "$(DEST)"
	@rsync -a --checksum --delete-excluded $(addprefix --include=/,$(PLUGIN_DIRS) $(PLUGIN_FILES)) --exclude='*' ./ "$(DEST)/"
	@echo "installed; enable with: omarchy plugin enable $(PLUGIN_ID)"

reload:
	@$(MAKE) --no-print-directory install
	@rm -rf "$(HOME)/.cache/quickshell/qmlcache"
	@omarchy restart shell

uninstall:
	@omarchy plugin disable $(PLUGIN_ID) 2>/dev/null || true
	@rm -rf "$(DEST)"
	@echo "removed"
