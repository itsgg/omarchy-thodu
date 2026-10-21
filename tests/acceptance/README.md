# Acceptance tests of record

Tests that decide whether a task is done live here, written when the task is written and before its code. They are protected (`tools/protected.txt`): changing one needs a `Protected: <path>: <why>` line in the commit and the coordinator's acceptance at landing, so a test of record is never edited to pass.
