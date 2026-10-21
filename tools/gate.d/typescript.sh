# TypeScript and JavaScript steps in every root with a package.json: types
# where there is a tsconfig.json, Biome where it is a dependency, and the
# package's own test script. Sourced by tools/gate.

typescript_turn() {
    each_root package.json tsc sh -c '[ ! -f tsconfig.json ] || npx --no-install tsc --noEmit'
    each_root package.json biome sh -c '[ ! -x node_modules/.bin/biome ] || npx --no-install biome ci .'
    each_root package.json npm-test npm test --silent
}

typescript_full() {
    each_root package.json npm-ls npm ls --depth=0
}

typescript_nightly() {
    :
}
