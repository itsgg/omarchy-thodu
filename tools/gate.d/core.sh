# Steps every project runs, whatever its stacks. Sourced by tools/gate.

core_turn() {
    step hooks checks hooks
    step conflicts checks conflicts
    step prose checks prose
    step protected checks protected
    step tests-removed checks tests-removed
    step symlinks checks symlinks
    step secrets checks secrets
    step agents-md checks agents-md
    step spec checks spec
    step tasks checks tasks
}

core_full() {
    step secrets-all checks secrets --all
    step acceptance checks tasks --run
}
