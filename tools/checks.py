"""The gate's checks and the hooks' logic, standard library only.

Run as `python3 -I tools/checks.py <check> [args]`. Each check prints what it
found and exits non-zero on a failure; an ERROR line says what to do. The
checks hold the rules AGENTS.md states, so a rule there names its check here.
"""

from __future__ import annotations

import fnmatch
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import unicodedata
from collections import Counter
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from functools import partial
from pathlib import Path

# CHECKS_ROOT lets the template repository run a check over its own files.
ROOT = Path(os.environ.get("CHECKS_ROOT") or Path(__file__).resolve().parent.parent)
MAIN = "main"

# Commit messages (rules.md, Git and Writing style).
SCISSORS = "# ------------------------ >8 ------------------------"
NAMED = r"(?:cursor|claude|fable|codex|sonnet|opus|haiku|gpt[\w.-]*|gemini|copilot|agy|ai)"
ACTOR = rf"(?:{NAMED}|(?:an?|the)\s+(?:agent|reviewer|model|llm|bot)|agents?|reviewers?)"
DONE = r"(?:written|authored|built|generated|drafted|assisted|made|created)"
CREDITS = (
    r"(?im)^\W*reviewed\s+by\b",
    r"(?im)^\W*review(?:ers?|s)?\s*:\s*(?!none\b|pending\b|n/?a\b)\S",
    r"(?im)\breview(?:ed)?\s+by\s+(?!hand\b|the\s+gate\b)\S",
    r"(?im)\breviewed\s+(?:it|this|the\s+(?:diff|change|commit|branch|patch|pull request))\b",
    rf"(?im)^\W*{DONE}\s+(?:by|with)\s+{ACTOR}\b",
    rf"(?im)\b(?:was|were|been)\s+{DONE}\s+(?:by|with)\s+{ACTOR}\b",
)
TRAILER = (
    r"(?im)^(?:[\w-]*(?:author|assist|generated?|review|help)[\w-]*-by|claude-session|made-with)\s*:"
    rf"|^[\w-]+-by\s*:[^\n]*\b{ACTOR}\b"
    r"|^\W*generated with \[?claude"
)
CONVENTIONAL = re.compile(
    r"^(?:feat|fix|test|docs|refactor|chore|build|ci|perf|style|revert)(?:\([a-z0-9./-]+\))?!?: \S"
)
IMPERATIVE = re.compile(r"^[A-Z][a-z]+(?:\s|$)")
DECLARED_PROTECTED = re.compile(r"(?m)^Protected:\s*(\S+?)\s*:\s*\S")
DECLARED_REMOVED = re.compile(r"(?m)^Removes test:\s*(\S+?)\s*:\s*\S")
DECLARED_MODIFIED = re.compile(r"(?m)^Modifies test:\s*(\S+?)\s*:\s*\S")

SECRETS = (
    (r"sk-ant-[a-z0-9]+-[A-Za-z0-9_\-]{40,}", "Anthropic key"),
    (r"\bsk-(?:proj-)?[A-Za-z0-9_\-]{32,}", "OpenAI key"),
    (r"\bAIza[0-9A-Za-z\-_]{35}\b", "Google API key"),
    (r"\b\d{8,11}:[A-Za-z0-9_\-]{35}\b", "Telegram bot token"),
    (r"\bgh[pousr]_[A-Za-z0-9]{36,}\b", "GitHub token"),
    (r"\bgithub_pat_[A-Za-z0-9_]{60,}\b", "GitHub token"),
    (r"\bglpat-[A-Za-z0-9_\-]{20,}\b", "GitLab token"),
    (r"\bxox[abprs]-[A-Za-z0-9-]{10,}\b", "Slack token"),
    (r"\bAKIA[0-9A-Z]{16}\b", "AWS access key"),
    (r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY(?: BLOCK)?-----", "private key"),
)

REQ = re.compile(r"\bREQ-[A-Z][A-Z0-9]*-\d+\b")
REQ_DEFINED = re.compile(r"(?m)^#{2,4}\s+(REQ-[A-Z][A-Z0-9]*-\d+)\b")
TASK_FILE = re.compile(r"^tasks/(\d{4})-[a-z0-9-]+\.md$")
TASK_STATUSES = {"open", "claimed", "review", "blocked", "done"}
TEXT_LIMIT = 2_000_000


class Fail(Exception):
    """A check failed; the message is printed after ERROR."""


def git(*args: str, check: bool = True, cwd: Path | None = None) -> str:
    """Run git in the repository and return its output."""
    result = subprocess.run(
        ["git", "-c", "core.quotePath=false", *args], cwd=cwd or ROOT, capture_output=True, text=True, check=False
    )
    if check and result.returncode != 0:
        raise Fail(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def gate_conf() -> dict[str, str]:
    """Read tools/gate.conf, then tools/gate.local.conf over it: shell files of NAME=value lines."""
    conf: dict[str, str] = {}
    for name in ("gate.conf", "gate.local.conf"):
        path = ROOT / "tools" / name
        if path.exists():
            for line in path.read_text(encoding="utf-8").splitlines():
                m = re.match(r"^([A-Z_][A-Z0-9_]*)=(.*)$", line.strip())
                if m:
                    conf[m.group(1)] = m.group(2).strip().strip("\"'")
    return conf


def files_listed(*args: str) -> list[str]:
    """Paths from a git listing command, read NUL-separated so no name is mangled."""
    split = args.index("--") if "--" in args else len(args)
    return [path for path in git(*args[:split], "-z", *args[split:]).split("\0") if path]


def tracked_and_new() -> list[str]:
    """Every file git tracks or would track: tracked plus untracked, not ignored."""
    return sorted(set(files_listed("ls-files", "--cached", "--others", "--exclude-standard")))


def read_text(path: Path) -> str | None:
    """A file's text, or None when it is missing, binary or too large."""
    try:
        if not path.is_file() or path.stat().st_size > TEXT_LIMIT:
            return None
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    return data.decode("utf-8", errors="replace")


def show(rev: str, path: str) -> str | None:
    """A file's text at a revision (":" for the index), or None if it was not there."""
    spec = f":{path}" if rev == ":" else f"{rev}:{path}"
    result = subprocess.run(["git", "show", spec], cwd=ROOT, capture_output=True, check=False)
    if result.returncode != 0 or b"\0" in result.stdout:
        return None
    return result.stdout.decode("utf-8", errors="replace")


def branch() -> str:
    return git("rev-parse", "--abbrev-ref", "HEAD").strip()


def ref_exists(ref: str) -> bool:
    return subprocess.run(
        ["git", "rev-parse", "-q", "--verify", ref], cwd=ROOT, capture_output=True, check=False
    ).returncode == 0


def base() -> str | None:
    """Where this branch's own commits start; None means the whole history.

    On a branch, its merge base with main (or origin/main); on main, with its
    upstream; with neither, every commit is checked. CI passes the pushed
    range's start in GATE_BASE, or the pull request's target."""
    env = os.environ.get("GATE_BASE", "").strip()
    if env:
        if set(env) <= {"0"} or not ref_exists(f"{env}^{{commit}}"):
            return None
        return git("rev-parse", env).strip()
    if os.environ.get("GATE_CI"):
        target = os.environ.get("GITHUB_BASE_REF")
        if target and ref_exists(f"refs/remotes/origin/{target}"):
            return git("merge-base", "HEAD", f"refs/remotes/origin/{target}").strip()
        return None
    if not ref_exists("HEAD"):
        return None
    if branch() != MAIN:
        for ref in (f"refs/heads/{MAIN}", f"refs/remotes/origin/{MAIN}"):
            if ref_exists(ref):
                return git("merge-base", "HEAD", ref).strip()
    upstream = subprocess.run(
        ["git", "rev-parse", "-q", "--verify", "@{upstream}"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    ).stdout.strip()
    if upstream:
        return git("merge-base", "HEAD", upstream).strip()
    return None


@dataclass
class Commit:
    sha: str
    parent: str | None
    files: list[str]
    message: str


def commits(since: str | None, head: str = "HEAD") -> list[Commit]:
    """The non-merge commits in since..head (all of head's history when None), oldest first."""
    if not ref_exists(head):
        return []
    spec = [f"{since}..{head}"] if since else [head]
    out = git("log", "--no-merges", "--reverse", "--no-renames", "--format=%x1e%H%x1f%P%x1f%B%x1f", "--name-only", *spec)
    found = []
    for record in out.split("\x1e")[1:]:
        sha, parents, message, rest = record.split("\x1f", 3)
        files = [line for line in rest.splitlines() if line.strip()]
        found.append(Commit(sha.strip(), parents.split()[0] if parents.split() else None, files, message))
    return found


def uncommitted_changes() -> list[str]:
    if not ref_exists("HEAD"):
        return tracked_and_new()
    changed = set(files_listed("diff", "--name-only", "--no-renames", "HEAD"))
    changed |= set(files_listed("ls-files", "--others", "--exclude-standard"))
    return sorted(changed)


def staged_changes() -> list[str]:
    return files_listed("diff", "--cached", "--name-only", "--no-renames")


def worktree_text(path: str) -> str | None:
    return read_text(ROOT / path)


def head_text(path: str) -> str | None:
    return show("HEAD", path)


def index_text(path: str) -> str | None:
    return show(":", path)


Reader = Callable[[str], "str | None"]


# Protected paths.

# Protected whatever tools/protected.txt says, so emptying that file in the
# same change protects nothing less.
FLOOR = (
    "SEED.md", "SPEC.md", "tools/**", ".githooks/**", ".claude/settings.json",
    ".github/workflows/**", ".copier-answers.yml", "tests/acceptance/**", "spec/cases/**",
)


def parse_patterns(text: str | None) -> set[str]:
    out = set()
    for line in (text or "").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            out.add(line)
    return out


def protected_patterns(*revs: str | None) -> list[str]:
    """The floor, the working tree's list, and the list at each given revision."""
    patterns = set(FLOOR) | parse_patterns(read_text(ROOT / "tools" / "protected.txt"))
    for rev in revs:
        if rev:
            patterns |= parse_patterns(show(rev, "tools/protected.txt"))
    return sorted(patterns)


def glob_regex(pattern: str) -> re.Pattern[str]:
    """A path glob: * and ? stay within one segment, ** crosses segments, and
    dir/** is everything under the directory (research/*/** works). The paths
    matched are files; hook_pre_tool also asks about "dir/" for a command
    that names a directory."""
    out = ""
    i = 0
    while i < len(pattern):
        if pattern.startswith("**", i):
            out += ".*"
            i += 2
        elif pattern[i] == "*":
            out += "[^/]*"
            i += 1
        elif pattern[i] == "?":
            out += "[^/]"
            i += 1
        else:
            out += re.escape(pattern[i])
            i += 1
    return re.compile(f"^{out}$")


def matches(pattern: str, path: str) -> bool:
    return glob_regex(pattern).match(path) is not None


def is_protected(path: str, patterns: list[str]) -> bool:
    return any(matches(p, path) for p in patterns)


def acceptance_block(text: str | None) -> str | None:
    """The Acceptance section of a task file, which only the coordinator may change."""
    if text is None:
        return None
    m = re.search(r"(?ms)^Acceptance:\s*\n?(.*?)(?=^## |^[A-Z][a-z]+( [a-z]+)*: |\Z)", text)
    return m.group(1).strip() if m else ""


def protected_touched(paths: list[str], old: Reader | None, new: Reader, patterns: list[str]) -> list[str]:
    """Protected paths among `paths`, plus task files whose Acceptance changed."""
    touched = [p for p in paths if is_protected(p, patterns)]
    if old is not None:
        for p in paths:
            if TASK_FILE.match(p):
                before = old(p)
                if before is not None and acceptance_block(before) != acceptance_block(new(p)):
                    touched.append(f"{p}#acceptance")
    return sorted(set(touched))


def undeclared(touched: list[str], declared: list[str]) -> list[str]:
    return [t for t in touched if not any(matches(d, t) or d == "*" for d in declared)]


def committed_problems(since: str | None, head: str = "HEAD") -> list[str]:
    """Each commit in since..head declares, in its own message, the protected
    paths it changes and the tests it removes or edits. The patterns are the
    union of the working tree's, HEAD's, head's and since's lists, so a branch
    can add to the list but never shorten it."""
    patterns = protected_patterns("HEAD", head, since)
    problems = []
    for c in commits(since, head):
        if c.parent is None:
            # The first commit founds the project (SEED.md alone); there is
            # nothing before it to weaken.
            continue
        at_commit = partial(show, c.sha)
        at_parent = partial(show, c.parent)
        for path in undeclared(protected_touched(c.files, at_parent, at_commit, patterns), DECLARED_PROTECTED.findall(c.message)):
            problems.append(f"{c.sha[:7]} changes {path} without a 'Protected: {path}: <why>' line in its message")
        for name in undeclared(removed_tests(c.files, at_parent, at_commit), DECLARED_REMOVED.findall(c.message)):
            problems.append(f"{c.sha[:7]} removes test {name} without a 'Removes test: {name}: <why>' line in its message")
        for name in undeclared(modified_tests(c.files, at_parent, at_commit), DECLARED_MODIFIED.findall(c.message)):
            problems.append(f"{c.sha[:7]} modifies test {name} without a 'Modifies test: {name}: <why>' line in its message")
    return problems


def check_declarations(argv: list[str]) -> None:
    """For landing: BASE..HEAD declares every protected change and every removed
    or edited test, judged by this checkout's checks and protected list, not
    the branch's own. Objects of an agent branch come through the alternate
    store tools/land exports."""
    if len(argv) != 2:
        raise Fail("declarations needs BASE and HEAD")
    for rev in argv:
        if not ref_exists(f"{rev}^{{commit}}"):
            raise Fail(f"{rev} is not a commit this repository can read")
    problems = committed_problems(git("rev-parse", argv[0]).strip(), git("rev-parse", argv[1]).strip())
    if problems:
        raise Fail("undeclared changes in " + argv[0] + ".." + argv[1] + ":\n  " + "\n  ".join(problems))
    print(f"declarations: every change in {argv[0]}..{argv[1]} is declared")


def check_protected() -> None:
    since = base()
    problems = [p for p in committed_problems(since) if " changes " in p]
    if not os.environ.get("GATE_IN_PRE_COMMIT"):
        old = head_text if ref_exists("HEAD") else None
        for path in protected_touched(uncommitted_changes(), old, worktree_text, protected_patterns("HEAD")):
            problems.append(f"{path} has an uncommitted change; revert it, or commit it with a 'Protected: {path}: <why>' line")
    if problems:
        raise Fail(
            "protected paths changed (tools/protected.txt). Weakening a check is never the fix; "
            "if the task needs this change, set the task to blocked and say why in its Log.\n  "
            + "\n  ".join(problems)
        )
    print(f"protected: every change since {since[:7] if since else 'the first commit'} is declared")


# Removed tests.

TEST_DIRS = {"tests", "test"}
JS_EXTENSIONS = {".js", ".mjs", ".cjs", ".jsx", ".ts", ".mts", ".cts", ".tsx"}
DEFINITIONS = {
    ".py": re.compile(r"(?m)^\s*(?:async\s+)?def\s+(test\w*)\s*\("),
    ".rs": re.compile(r"#\[(?:[\w:]+::)?test\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)"),
    ".go": re.compile(r"(?m)^func\s+((?:Test|Fuzz|Benchmark|Example)\w*)\s*\("),
    ".js": re.compile(r"""\b(?:it|test)\s*\(\s*(['"`])((?:(?!\1).)+)\1"""),
    ".qml": re.compile(r"(?m)^\s*function\s+(test_\w+)\s*\("),
}


def is_test_file(path: str) -> bool:
    """Where each language keeps tests, plus any file under a tests/ or test/
    directory, plus the TEST_GLOBS of tools/gate.local.conf."""
    parts = path.split("/")
    name = parts[-1]
    ext = os.path.splitext(name)[1]
    in_test_dir = any(part in TEST_DIRS for part in parts[:-1])
    if any(fnmatch.fnmatchcase(path, g) for g in gate_conf().get("TEST_GLOBS", "").split()):
        return True
    if ext == ".rs":
        return True
    if ext == ".go":
        return name.endswith("_test.go")
    if ext == ".qml":
        return name.startswith("tst_")
    if ext == ".py":
        return name.startswith("test_") or name.endswith("_test.py") or in_test_dir
    if ext in JS_EXTENSIONS:
        return bool(re.search(r"\.(?:test|spec)\.", name)) or in_test_dir
    return False


def test_names(path: str, text: str | None) -> list[str]:
    if text is None:
        return []
    ext = os.path.splitext(path)[1]
    pattern = DEFINITIONS[".js"] if ext in JS_EXTENSIONS else DEFINITIONS.get(ext)
    if pattern is None:
        return []
    return [f[-1] if isinstance(f, tuple) else f for f in pattern.findall(text)]


def removed_tests(paths: list[str], old_text: Reader, new_text: Reader) -> list[str]:
    old: Counter[str] = Counter()
    new: Counter[str] = Counter()
    for p in paths:
        if is_test_file(p):
            old.update(test_names(p, old_text(p)))
            new.update(test_names(p, new_text(p)))
    return sorted((old - new).elements())


def body_end(text: str, ext: str, start: int, def_end: int) -> int:
    """Where the test that starts at `start` ends: its closing brace in brace
    languages, the next line indented no deeper than its def in Python."""
    if ext == ".py":
        line_start = text.rfind("\n", 0, start) + 1
        indent = len(text[line_start:start]) - len(text[line_start:start].lstrip())
        pos = text.find("\n", def_end)
        while pos != -1:
            nxt = text.find("\n", pos + 1)
            line = text[pos + 1 : nxt if nxt != -1 else len(text)]
            if line.strip() and len(line) - len(line.lstrip()) <= indent and not line.lstrip().startswith((")", "]", "}")):
                return pos
            pos = nxt
        return len(text)
    brace = text.find("{", def_end)
    if brace == -1:
        return len(text)
    depth = 0
    for i in range(brace, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(text)


def folded(body: str, ext: str) -> str:
    """A test body with what formatters change taken out: comments, whitespace,
    trailing commas and, outside Rust, the kind of quote."""
    comment = r"(?m)(?:^|\s)#.*$" if ext == ".py" else r"(?m)(?:^|\s)//.*$"
    body = re.sub(comment, "", body)
    body = re.sub(r"\s+", "", body)
    body = re.sub(r",(?=[)\]}])", "", body)
    if ext != ".rs":
        body = body.replace("'", '"')
    return body


def decorated_start(text: str, start: int) -> int:
    """The start of the decorators above a Python def at `start`, which belong
    to its test; a decorator may run over several lines."""
    lines = text[:start].split("\n")[:-1]
    def_line_start = text.rfind("\n", 0, start) + 1
    indent = start - def_line_start if text[def_line_start:start].strip() == "" else 0
    best = start
    offset = def_line_start
    for line in reversed(lines):
        offset -= len(line) + 1
        stripped = line.strip()
        if not stripped:
            break
        depth = len(line) - len(line.lstrip())
        if depth == indent and stripped.startswith("@"):
            best = offset
        elif depth > indent or stripped.startswith((")", "]", "}")):
            continue
        else:
            break
    return best


def test_bodies(path: str, text: str | None) -> dict[str, list[str]]:
    """Each test's body, folded, by name; tests sharing a name (the same it()
    title in two describe blocks) are kept in order."""
    if text is None:
        return {}
    ext = os.path.splitext(path)[1]
    pattern = DEFINITIONS[".js"] if ext in JS_EXTENSIONS else DEFINITIONS.get(ext)
    if pattern is None:
        return {}
    bodies: dict[str, list[str]] = {}
    for m in pattern.finditer(text):
        name = m.group(m.lastindex or 0)
        # The pattern's leading \s* may take blank lines; start at the keyword.
        keyword = m.start() + len(m.group(0)) - len(m.group(0).lstrip())
        start = decorated_start(text, keyword) if ext == ".py" else keyword
        end = body_end(text, ext, keyword, m.end())
        bodies.setdefault(name, []).append(folded(text[start:end], ext))
    return bodies


def modified_tests(paths: list[str], old_text: Reader, new_text: Reader) -> list[str]:
    """Tests that exist before and after a change with a different body: an
    assertion edited, a tolerance loosened, a case special-cased. Adding a
    test is free; a change to one that already passed needs saying why."""
    changed: set[str] = set()
    for p in paths:
        if is_test_file(p):
            before = test_bodies(p, old_text(p))
            after = test_bodies(p, new_text(p))
            # Same count under a name: compare in order. A different count is
            # a removal (removed_tests) or an addition (free).
            changed.update(
                name for name, bodies in before.items()
                if len(after.get(name, [])) == len(bodies) and after[name] != bodies
            )
    return sorted(changed)


def check_tests_removed() -> None:
    since = base()
    problems = [p for p in committed_problems(since) if " removes test " in p or " modifies test " in p]
    if not os.environ.get("GATE_IN_PRE_COMMIT") and ref_exists("HEAD"):
        changes = uncommitted_changes()
        for name in removed_tests(changes, head_text, worktree_text):
            problems.append(f"test {name} removed in uncommitted work; restore it, or commit with a 'Removes test: {name}: <why>' line")
        for name in modified_tests(changes, head_text, worktree_text):
            problems.append(f"test {name} changed in uncommitted work; restore it, or commit with a 'Modifies test: {name}: <why>' line")
    if problems:
        raise Fail("tests disappeared or changed. Deleting, renaming or editing a test to pass is never the fix.\n  " + "\n  ".join(problems))
    print("tests-removed: none undeclared")


GUARDED_DIRS = {"tasks", "tests", "test", "spec", "tools", ".githooks"}


def check_symlinks() -> None:
    """Task, test, spec and tool files are regular files, so what a check reads
    is what a change to that path changes."""
    found = [rel for rel in tracked_and_new() if rel.split("/")[0] in GUARDED_DIRS and (ROOT / rel).is_symlink()]
    if found:
        raise Fail("symbolic links where checks read files; replace them with the files: " + ", ".join(found))
    print("symlinks: none in guarded directories")


# Commit messages.

def message_problems(message: str, style: str) -> list[str]:
    message = re.split(rf"(?m)^{re.escape(SCISSORS)}$", message, maxsplit=1)[0]
    lines = [line for line in message.splitlines() if not line.startswith("#")]
    while lines and not lines[-1].strip():
        lines.pop()
    prose = "\n".join(lines)
    found: list[str] = []
    if not lines or not lines[0].strip():
        return ["an empty subject"]
    subject = lines[0]
    if subject.startswith(("Merge ", "fixup! ", "squash! ", "amend! ")):
        return found
    if not prose.isascii():
        found.append("a non-ASCII character (an em dash, a curly quote, an arrow)")
    if re.search(r"\S - \S", prose):
        found.append("a spaced hyphen used as a dash")
    if len(lines) > 1 and lines[1].strip():
        found.append("no blank line after the subject")
    if re.search(TRAILER, message):
        found.append("an attribution trailer")
    elif any(re.search(pattern, message) for pattern in CREDITS):
        found.append("a line crediting the change or its review to an agent, a model or a reviewer")
    if style == "conventional" and not CONVENTIONAL.match(subject):
        found.append("a subject that is not Conventional Commits (feat:, fix:, test:, docs:, refactor:, chore:, ...)")
    if style == "imperative" and (not IMPERATIVE.match(subject) or CONVENTIONAL.match(subject)):
        found.append("a subject that is not an imperative sentence starting with a capital (Add ..., Fix ...)")
    if subject.endswith("."):
        found.append("a full stop at the end of the subject")
    return found


def check_commit_msg(argv: list[str]) -> None:
    if not argv:
        raise Fail("commit-msg needs the message file")
    message = Path(argv[0]).read_text(encoding="utf-8")
    style = gate_conf().get("SUBJECT_STYLE", "conventional")
    found = message_problems(message, style)
    staged = staged_changes()
    has_head = ref_exists("HEAD")
    patterns = protected_patterns("HEAD" if has_head else None, ":")
    touched = protected_touched(staged, head_text if has_head else None, index_text, patterns)
    for path in undeclared(touched, DECLARED_PROTECTED.findall(message)):
        found.append(f"a change to protected {path} without a 'Protected: {path}: <why>' line")
    if has_head:
        gone = removed_tests(staged, head_text, index_text)
        for name in undeclared(gone, DECLARED_REMOVED.findall(message)):
            found.append(f"a removed test {name} without a 'Removes test: {name}: <why>' line")
        for name in undeclared(modified_tests(staged, head_text, index_text), DECLARED_MODIFIED.findall(message)):
            found.append(f"an edited test {name} without a 'Modifies test: {name}: <why>' line")
    if found:
        raise Fail("commit message: " + "\n  commit message: ".join(found))


# Repository hygiene.

def check_hooks() -> None:
    if os.environ.get("GATE_CI"):
        print("hooks: skipped on CI")
        return
    local = git("config", "--local", "--get", "core.hooksPath", check=False).strip()
    if local:
        raise Fail(f"a local core.hooksPath ({local}) hides the hooks; run: git config --local --unset core.hooksPath")
    common = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir").strip())
    missing = []
    for hook in sorted((ROOT / ".githooks").glob("*")):
        shim = common / "hooks" / hook.name
        if read_text(shim) != hook_shim(hook.name) or not os.access(shim, os.X_OK):
            missing.append(hook.name)
    if missing:
        raise Fail(f"hooks not installed: {', '.join(missing)}; run: tools/install-hooks")
    # With a global core.hooksPath, ask git to run each hook as a commit
    # would, with a one-time token only this check knows, so a hooks path that
    # does not dispatch here is caught whatever its scripts look like.
    effective = git("config", "--get", "core.hooksPath", check=False).strip()
    if effective:
        probe_hooks(effective)
    print("hooks: installed")


def probe_hooks(effective: str) -> None:
    gate_dir = ROOT / ".gate"
    gate_dir.mkdir(exist_ok=True)
    token = hashlib.sha256(os.urandom(16)).hexdigest()[:24]
    token_file, message = gate_dir / "hook-probe-token", gate_dir / "hook-probe-msg"
    token_file.write_text(token, encoding="utf-8")
    message.write_text("chore: probe\n", encoding="utf-8")
    timeout = int(os.environ.get("HOOK_PROBE_TIMEOUT", "30"))
    try:
        for hook in sorted((ROOT / ".githooks").glob("*")):
            args = ["--", str(message)] if hook.name == "commit-msg" else []
            try:
                ran = subprocess.run(
                    ["git", "hook", "run", "--ignore-missing", hook.name, *args], cwd=ROOT,
                    env={**os.environ, "GATE_HOOK_PROBE": token}, stdin=subprocess.DEVNULL,
                    capture_output=True, text=True, check=False, timeout=timeout,
                )
            except subprocess.TimeoutExpired as e:
                raise Fail(f"core.hooksPath is {effective}, and its {hook.name} did not finish in {timeout}s") from e
            if "is not a git command" in ran.stderr:
                print(f"hooks: git has no `git hook run`, so whether {effective} dispatches here is not checked")
                return
            if ran.returncode != 0 or f"{HOOK_PROBE_ANSWER} {token}" not in ran.stdout + ran.stderr:
                raise Fail(
                    f"core.hooksPath is {effective}, and it does not run this repository's {hook.name} hook "
                    f"(exit {ran.returncode}); make it dispatch to $(git rev-parse --git-common-dir)/hooks, "
                    "or unset it, then run tools/install-hooks"
                )
    finally:
        token_file.unlink(missing_ok=True)
        message.unlink(missing_ok=True)


HOOK_PROBE_ANSWER = "vithai-hook-probe-ok"


def hook_shim(name: str) -> str:
    """What tools/install-hooks writes into .git/hooks: run the worktree's own
    copy, so a sandboxed worktree that cannot see the main checkout runs its hooks."""
    return (
        "#!/bin/sh\n"
        "# Installed by tools/install-hooks; runs this worktree's .githooks copy.\n"
        f'exec "$(git rev-parse --show-toplevel)/.githooks/{name}" "$@"\n'
    )


def check_conflicts() -> None:
    marker = re.compile(r"(?m)^(?:<{7}|>{7})(?: |$)")
    found = []
    for rel in tracked_and_new():
        text = read_text(ROOT / rel)
        if text and marker.search(text):
            found.append(rel)
    if found:
        raise Fail("merge conflict markers left in: " + ", ".join(found))
    print("conflicts: none")


def prose_lines(text: str) -> Iterator[tuple[int, str]]:
    """Lines of markdown outside fenced code, with inline code removed."""
    fence = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith(("```", "~~~")):
            fence = not fence
            continue
        if not fence:
            yield number, re.sub(r"`[^`]*`", "", line)


def odd_character(line: str) -> str | None:
    """The first non-ASCII character that is not a letter or an accent: a
    dash, a curly quote, an arrow, an emoji. A name keeps its spelling."""
    for ch in line:
        if not ch.isascii() and unicodedata.category(ch) not in NAME_CATEGORIES:
            return ch
    return None


# Letters and accents; not Lm, the modifier letters, which hold look-alike
# apostrophes and quotes (U+02BC).
NAME_CATEGORIES = {"Lu", "Ll", "Lt", "Lo", "Mn", "Mc"}


def check_prose() -> None:
    excluded = gate_conf().get("PROSE_EXCLUDE", "").split()
    found = []
    for rel in tracked_and_new():
        if not rel.endswith(".md") or rel.startswith(("node_modules/", ".venv/")):
            continue
        if any(matches(g, rel) for g in excluded):
            continue
        text = read_text(ROOT / rel)
        if text is None:
            continue
        for number, line in prose_lines(text):
            odd = odd_character(line)
            if odd is not None:
                found.append(f"{rel}:{number}: {unicodedata.name(odd, 'a non-ASCII character').lower()}")
            elif re.search(r"\S - \S", line):
                found.append(f"{rel}:{number}: a spaced hyphen used as a dash")
    if found:
        raise Fail("prose is plain ASCII with no dash stand-ins:\n  " + "\n  ".join(found[:40]))
    print(f"prose: plain{', ' + str(len(excluded)) + ' pattern(s) excluded' if excluded else ''}")


def check_secrets(argv: list[str]) -> None:
    every = "--all" in argv
    since = base()
    if every or since is None:
        paths = tracked_and_new()
    else:
        changed = {f for c in commits(since) for f in c.files} | set(uncommitted_changes())
        paths = sorted(p for p in changed if (ROOT / p).exists())
    found = []
    for rel in paths:
        text = read_text(ROOT / rel)
        if text is None:
            continue
        for pattern, label in SECRETS:
            for m in re.finditer(pattern, text):
                line = text.count("\n", 0, m.start()) + 1
                found.append(f"{rel}:{line}: {label}")
    if every and since is not None and shutil.which("betterleaks"):
        diff = git("diff", since)
        result = subprocess.run(
            ["betterleaks", "stdin", "--no-banner", "--redact"], input=diff,
            capture_output=True, text=True, check=False,
        )
        if result.returncode != 0:
            found.append("betterleaks found a secret in the branch diff:\n" + result.stdout[-2000:])
    if found:
        raise Fail("possible secrets; remove them and rotate any that were real:\n  " + "\n  ".join(found))
    print(f"secrets: none in {len(paths)} file(s)")


def check_agents_md() -> None:
    path = ROOT / "AGENTS.md"
    if not path.exists():
        raise Fail("AGENTS.md is missing")
    text = path.read_text(encoding="utf-8")
    limit = int(gate_conf().get("AGENTS_MD_MAX_LINES", "150"))
    count = len(text.splitlines())
    problems = []
    if count > limit:
        problems.append(f"AGENTS.md has {count} lines; keep it under {limit} and move rules into checks")
    section = re.search(r"(?ms)^## Commands\s*$(.*?)(?=^## |\Z)", text)
    commands = re.findall(r"(?m)^\s*-\s*`([^`]+)`", section.group(1)) if section else []
    if not commands:
        problems.append("AGENTS.md has no '## Commands' section of '- `command`' lines")
    for command in commands:
        words = [w for w in command.split() if not re.match(r"^[A-Z_][A-Z0-9_]*=", w)]
        if not words:
            continue
        word = words[0]
        if "/" in word:
            target = ROOT / word
            if not target.exists() or not os.access(target, os.X_OK):
                problems.append(f"`{command}`: {word} is not an executable file")
        elif shutil.which(word) is None and word not in {"cd", "source", "."}:
            problems.append(f"`{command}`: {word} is not on PATH")
    if problems:
        raise Fail("AGENTS.md does not match the repository:\n  " + "\n  ".join(problems))
    print(f"agents-md: {len(commands)} command(s) exist, {count} lines")


# Spec traceability.

def spec_ids() -> tuple[set[str], set[str], list[str]]:
    """Live and retired requirement IDs in SPEC.md, and duplicate definitions."""
    text = read_text(ROOT / "SPEC.md") or ""
    live: list[str] = []
    retired: set[str] = set()
    in_retired = False
    for line in text.splitlines():
        heading = re.match(r"^(#{1,4})\s+(.*)$", line)
        if heading:
            in_retired = "retired" in heading.group(2).lower()
            m = REQ_DEFINED.match(line)
            if m and not in_retired:
                live.append(m.group(1))
            continue
        if in_retired:
            retired.update(REQ.findall(line))
    dupes = sorted(i for i, n in Counter(live).items() if n > 1)
    return set(live), retired, dupes


@dataclass
class Task:
    status: str
    depends: list[str]
    spec: list[str]
    runs: list[str]


def parse_task(text: str) -> Task:
    def field(name: str) -> str:
        m = re.search(rf"(?m)^{name}:\s*(.*)$", text)
        return m.group(1).strip() if m else ""

    return Task(
        status=field("Status").lower(),
        depends=re.findall(r"\b\d{4}\b", field("Depends on")),
        spec=REQ.findall(field("Spec")),
        runs=re.findall(r"(?m)^\s*-\s*run:\s*`([^`]+)`", acceptance_block(text) or ""),
    )


def tasks() -> dict[str, tuple[str, Task]]:
    out = {}
    for rel in tracked_and_new():
        m = TASK_FILE.match(rel)
        if m:
            text = read_text(ROOT / rel) or ""
            out[m.group(1)] = (rel, parse_task(text))
    return out


def check_spec() -> None:
    if not (ROOT / "SPEC.md").exists():
        print("spec: no SPEC.md yet")
        return
    live, retired, dupes = spec_ids()
    problems = [f"{i} is defined more than once in SPEC.md" for i in dupes]
    skip = ("SPEC.md", "SEED.md", "AGENTS.md", "README.md")
    referenced: Counter[str] = Counter()
    for rel in tracked_and_new():
        if rel in skip or rel.startswith(("changes/", "tasks/", ".gate/")):
            continue
        text = read_text(ROOT / rel)
        if not text:
            continue
        for ref in set(REQ.findall(text)):
            if is_test_file(rel):
                referenced[ref] += 1
            if ref in retired:
                problems.append(f"{rel} cites {ref}, which SPEC.md retired")
            elif ref not in live:
                problems.append(f"{rel} cites {ref}, which SPEC.md does not define")
    for number, (rel, task) in sorted(tasks().items()):
        if task.status in {"review", "done"}:
            for ref in task.spec:
                if ref not in live:
                    problems.append(f"{rel} cites {ref}, which SPEC.md does not define")
                elif not referenced[ref]:
                    problems.append(f"{rel} is {task.status} but no test file cites {ref}")
    if problems:
        raise Fail("spec traceability:\n  " + "\n  ".join(problems))
    covered = sum(1 for i in live if referenced[i])
    print(f"spec: {len(live)} requirement(s), {covered} with tests, {len(retired)} retired")


def check_tasks(argv: list[str]) -> None:
    run = "--run" in argv
    found = tasks()
    problems = []
    for number, (rel, task) in sorted(found.items()):
        if task.status not in TASK_STATUSES:
            problems.append(f"{rel}: Status must be one of {', '.join(sorted(TASK_STATUSES))}")
        for dep in task.depends:
            if dep not in found:
                problems.append(f"{rel}: depends on {dep}, which has no task file")
        if task.status in {"review", "done"} and not task.runs:
            problems.append(f"{rel}: {task.status} with no '- run: `command`' line under Acceptance")
    if run:
        for number, (rel, task) in sorted(found.items()):
            if task.status not in {"review", "done"}:
                continue
            for command in task.runs:
                result = subprocess.run(
                    ["bash", "-c", command], cwd=ROOT, capture_output=True, text=True, check=False, timeout=1800
                )
                if result.returncode != 0:
                    tail = (result.stdout + result.stderr).strip().splitlines()[-10:]
                    problems.append(f"{rel} is {task.status} but `{command}` fails:\n    " + "\n    ".join(tail))
                else:
                    print(f"tasks: {number} `{command}` passes")
    if problems:
        raise Fail("tasks:\n  " + "\n  ".join(problems))
    print(f"tasks: {len(found)} task file(s) well formed")


def check_manifest() -> None:
    path = ROOT / "manifest.json"
    if not path.exists():
        print("manifest: none")
        return
    try:
        m = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as e:
        raise Fail(f"manifest.json is not JSON: {e}") from e
    problems = []
    if m.get("schemaVersion") != 1:
        problems.append("schemaVersion must be the number 1")
    if not re.match(r"^io\.github\.[a-z0-9-]+\.[a-z0-9-]+$", str(m.get("id", ""))):
        problems.append("id must be io.github.<user>.<plugin>")
    for key in ("name", "version", "author", "description", "kinds", "entryPoints"):
        if not m.get(key):
            problems.append(f"missing {key}")
    if not re.match(r"^\d+\.\d+\.\d+$", str(m.get("version", ""))):
        problems.append("version must be semver")
    for kind, entry in (m.get("entryPoints") or {}).items():
        if not (ROOT / entry).exists():
            problems.append(f"entry point for {kind} is missing: {entry}")
    if problems:
        raise Fail("manifest.json: " + "; ".join(problems))
    print("manifest: ok")


# Reviewer output.

def stream_text(raw: str) -> str:
    """Join a reviewer's answer from stream-json output (Cursor, Claude Code or agy).

    Plain lines (reconnect notices) are skipped; when the joined assistant
    text is longer than the final result, it wins."""
    texts: list[str] = []
    deltas: list[str] = []
    result = ""
    for line in raw.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") == "assistant":
            for block in (event.get("message") or {}).get("content") or []:
                if isinstance(block, dict) and block.get("type") == "text":
                    texts.append(block.get("text", ""))
        elif event.get("type") == "result" and isinstance(event.get("result"), str):
            result = event["result"]
        elif event.get("event") == "step_update":
            step = event.get("step_update") or {}
            if step.get("step_type") == "agent_response" and step.get("text_delta"):
                deltas.append(step["text_delta"])
        elif event.get("event") == "result":
            response = (event.get("result") or {}).get("response")
            if isinstance(response, str):
                result = response
    joined = "\n".join(t for t in texts if t.strip()) or "".join(deltas)
    return joined if len(joined.strip()) > len(result.strip()) else result


# Claude Code hooks.

def file_digest(path: Path) -> bytes:
    if not path.is_file():
        return b""
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").digest()


def fingerprint() -> str:
    """What the gate's turn tier depends on: HEAD, every uncommitted change in
    full, and the installed hook shims."""
    h = hashlib.sha256()
    h.update(git("rev-parse", "HEAD", check=False).encode())
    h.update(git("diff", "HEAD", "--binary", check=False).encode())
    for rel in files_listed("ls-files", "--others", "--exclude-standard"):
        h.update(rel.encode() + b"\0" + file_digest(ROOT / rel))
    common = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir").strip())
    for shim in sorted((common / "hooks").glob("*")):
        h.update(shim.name.encode() + b"\0" + file_digest(shim))
    return h.hexdigest()


def hook_stop() -> int:
    """Stop hook: run the turn gate when the tree changed since it last passed.

    A block decision sends Claude back to work with the failure; after three
    tries in a row on one session it lets the turn end, and the commit hook
    still holds."""
    try:
        payload = json.loads(sys.stdin.read() or "{}")
    except json.JSONDecodeError:
        payload = {}
    state = ROOT / ".gate"
    state.mkdir(exist_ok=True)
    print_now = fingerprint()
    green = state / "last-green"
    if green.exists() and green.read_text().strip() == print_now:
        return 0
    session = re.sub(r"[^A-Za-z0-9-]", "", str(payload.get("session_id", "none")))[:64]
    tries_file = state / f"stop-{session}"
    tries = int(tries_file.read_text()) if tries_file.exists() and payload.get("stop_hook_active") else 0
    result = subprocess.run([str(ROOT / "tools" / "gate"), "--turn"], cwd=ROOT, capture_output=True, text=True, check=False)
    if result.returncode == 0:
        green.write_text(fingerprint())
        tries_file.unlink(missing_ok=True)
        return 0
    if tries >= 3:
        tries_file.unlink(missing_ok=True)
        print("tools/gate --turn still fails after three tries; the commit hook will refuse a commit until it passes.", file=sys.stderr)
        return 0
    tries_file.write_text(str(tries + 1))
    tail = "\n".join(result.stdout.strip().splitlines()[-30:])
    # A block on stdout, not exit 2: Claude Code lets an exit-2 Stop hook
    # through as a non-blocking error when its stderr mentions a missing file,
    # and a failing test often does.
    print(json.dumps({"decision": "block", "reason": f"tools/gate --turn failed. Fix the cause, or set your task to blocked:\n{tail}"}))
    return 0


WRITES = re.compile(r"(?:>|\btee\b|\bsed\s+-[a-zA-Z]*i|\bperl\s+-[a-zA-Z]*i|\bmv\b|\bcp\b|\brm\b|\btruncate\b|\bchmod\b|\bln\b|\bdd\b|\binstall\b)")


def ask(reason: str, decision: str = "ask") -> int:
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision,
            "permissionDecisionReason": reason
            + " Agents do not change it; if your task needs it, set the task to blocked and say why.",
        }
    }))
    return 0


def relative(target: str, cwd: Path) -> str | None:
    path = Path(target)
    if not path.is_absolute():
        path = cwd / path
    try:
        return path.resolve().relative_to(ROOT.resolve()).as_posix()
    except ValueError:
        return None


def edited(current: str, tool: str, args: dict[str, object]) -> str | None:
    """A file's content after a Write, Edit or MultiEdit, or None if it cannot be told."""
    if tool == "Write":
        content = args.get("content")
        return content if isinstance(content, str) else None
    edits = args.get("edits") if tool == "MultiEdit" else [args]
    text = current
    for edit in edits if isinstance(edits, list) else []:
        if not isinstance(edit, dict):
            return None
        old, new = edit.get("old_string"), edit.get("new_string")
        if not isinstance(old, str) or not isinstance(new, str):
            return None
        text = text.replace(old, new) if edit.get("replace_all") else text.replace(old, new, 1)
    return text


def hook_pre_tool() -> int:
    """PreToolUse hook: ask before a tool writes a protected path or a task's
    Acceptance. Shell commands are read with a heuristic; the gate, the
    commit-msg hook and landing are what hold the rule."""
    try:
        payload = json.loads(sys.stdin.read() or "{}")
    except json.JSONDecodeError:
        return 0
    tool = str(payload.get("tool_name") or "")
    args = payload.get("tool_input") or {}
    if not isinstance(args, dict):
        return 0
    cwd = Path(str(payload.get("cwd") or ROOT))
    # With permission prompts off (a build agent in its sandbox) nobody can
    # answer an ask, and only a deny is documented to hold in that mode.
    decision = "deny" if payload.get("permission_mode") == "bypassPermissions" else "ask"
    answer = partial(ask, decision=decision)
    patterns = protected_patterns("HEAD" if ref_exists("HEAD") else None)
    if tool == "Bash":
        command = str(args.get("command") or "")
        if WRITES.search(command):
            for token in re.findall(r"[\w./@+-]+", command):
                rel = relative(token, cwd)
                if rel and rel != "." and (is_protected(rel, patterns) or is_protected(rel + "/", patterns)):
                    return answer(f"This command may write {rel}, which is protected (tools/protected.txt).")
        return 0
    target = args.get("file_path") or args.get("notebook_path")
    if not isinstance(target, str):
        return 0
    rel = relative(target, cwd)
    if rel is None:
        return 0
    if is_protected(rel, patterns):
        return answer(f"{rel} is protected (tools/protected.txt).")
    if TASK_FILE.match(rel):
        current = read_text(ROOT / rel)
        if current is not None:
            after = edited(current, tool, args)
            if after is None or acceptance_block(after) != acceptance_block(current):
                return answer(f"This changes the Acceptance of {rel}, which is protected.")
    return 0


PROMPT_LIMIT = 120_000
REVIEW_HEADER = (
    "Do the review yourself now and give it as your reply: do not write a plan, do not call the plan tool "
    "and do not hand the work to subagents. Do not run any command; open a file only when the diff below "
    "is not enough. The files in your workspace are an export of the change's head, with no git repository."
)


def capped(text: str, limit: int) -> str:
    data = text.encode()
    if len(data) <= limit:
        return text
    return data[:limit].decode(errors="ignore") + f"\n[{len(data) - limit} more bytes not shown]\n"


def review_prompt(argv: list[str]) -> None:
    """The review prompt for BASE..HEAD within one command-line argument:
    the instructions, the commits, the changed task files and the gate's
    summary, each capped, then the diff if it fits, else its file list."""
    if len(argv) < 2:
        raise Fail("review-prompt needs BASE and HEAD, and optionally the gate summary file")
    start, head = argv[0], argv[1]
    summary = Path(argv[2]) if len(argv) > 2 else None
    parts = [REVIEW_HEADER, (ROOT / "tools" / "review-prompt.md").read_text(encoding="utf-8")]
    parts.append("## Commits\n" + capped(git("log", "--format=%h %s%n%b", f"{start}..{head}"), 20_000))
    tasks_text = ""
    for task in files_listed("diff", "--name-only", start, head, "--", "tasks/[0-9]*.md"):
        tasks_text += f"## {task}\n" + capped(show(head, task) or "(deleted)\n", 8_000)
    if tasks_text:
        parts.append(capped(tasks_text, 30_000))
    if summary is not None and summary.is_file() and not summary.is_symlink():
        parts.append("## Gate\n" + capped(summary.read_text(encoding="utf-8", errors="replace"), 10_000))
    fixed = "\n\n".join(parts)
    diff = git("diff", start, head)
    listing = git("diff", "--stat=200", start, head)
    if len((fixed + diff).encode()) + 100 <= PROMPT_LIMIT:
        prompt = fixed + "\n\n## Diff\n" + diff
    else:
        prompt = (
            fixed + f"\n\n## Diff\nThe diff is {len(diff.encode())} bytes, too long to include. "
            "These files changed; read them in your workspace:\n" + listing
        )
    if len(prompt.encode()) > PROMPT_LIMIT:
        raise Fail(f"the review prompt is {len(prompt.encode())} bytes even without the diff; review a smaller change")
    sys.stdout.write(prompt)


def export(argv: list[str]) -> None:
    """Write REV's tree into DIR exactly as committed, for a reviewer to read:
    every blob byte for byte (no .gitattributes export rules apply), and a
    symbolic link as a small text file naming its target, so nothing in the
    export points outside it."""
    if len(argv) != 2:
        raise Fail("export needs REV and DIR")
    rev, dest = argv[0], Path(argv[1])
    entries = []
    for entry in git("ls-tree", "-r", "-z", "--full-tree", rev).split("\0"):
        if not entry:
            continue
        meta, path = entry.split("\t", 1)
        mode, kind, sha = meta.split()
        parts = Path(path).parts
        if kind != "blob" or Path(path).is_absolute() or ".." in parts:
            continue
        entries.append((mode, sha, path))
    batch = subprocess.run(
        ["git", "cat-file", "--batch"], cwd=ROOT, input="".join(f"{sha}\n" for _, sha, _ in entries).encode(),
        capture_output=True, check=True,
    ).stdout
    offset = 0
    for mode, _, path in entries:
        header_end = batch.index(b"\n", offset)
        size = int(batch[offset:header_end].split()[2])
        data = batch[header_end + 1 : header_end + 1 + size]
        offset = header_end + 1 + size + 1
        target = dest / path
        target.parent.mkdir(parents=True, exist_ok=True)
        if mode == "120000":
            target.write_text(f"(a symbolic link to {data.decode(errors='replace')})\n", encoding="utf-8")
        else:
            target.write_bytes(data)
    print(f"export: {len(entries)} file(s)")


SKIP_DIRS = {"node_modules", "target", ".venv", "vendor", ".gate", ".git"}


def roots(argv: list[str]) -> None:
    """The directories holding a manifest, one per line; with --outermost, only
    those not inside another (a Cargo workspace runs once, at its root)."""
    if not argv:
        raise Fail("roots needs a manifest name")
    manifest = argv[0]
    found = sorted({
        str(Path(rel).parent) for rel in tracked_and_new()
        if Path(rel).name == manifest and not SKIP_DIRS & set(Path(rel).parts)
    })
    if "--outermost" in argv:
        kept: list[str] = []
        for d in found:
            if not any(k == "." or d.startswith(k + "/") for k in kept):
                kept.append(d)
        found = kept
    for d in found:
        print(d)


def declared(argv: list[str]) -> None:
    """List the Protected and Removes test lines in BASE..HEAD, for landing."""
    since = argv[0] if argv else base()
    text = "\n".join(c.message for c in commits(since))
    for path in DECLARED_PROTECTED.findall(text):
        print(f"protected {path}")
    for name in DECLARED_REMOVED.findall(text):
        print(f"removes-test {name}")


CHECKS = {
    "commit-msg": check_commit_msg,
    "hooks": lambda argv: check_hooks(),
    "conflicts": lambda argv: check_conflicts(),
    "prose": lambda argv: check_prose(),
    "protected": lambda argv: check_protected(),
    "tests-removed": lambda argv: check_tests_removed(),
    "symlinks": lambda argv: check_symlinks(),
    "secrets": check_secrets,
    "agents-md": lambda argv: check_agents_md(),
    "spec": lambda argv: check_spec(),
    "tasks": check_tasks,
    "manifest": lambda argv: check_manifest(),
    "declared": declared,
    "declarations": check_declarations,
    "roots": roots,
    "review-prompt": review_prompt,
    "export": export,
}


def main(argv: list[str]) -> int:
    if not argv:
        print("usage: tools/checks.py <" + "|".join([*CHECKS, "stream-text", "hook-stop", "hook-pre-tool"]) + "> [args]")
        return 2
    name, rest = argv[0], argv[1:]
    if name == "stream-text":
        print(stream_text(sys.stdin.read()))
        return 0
    if name == "hook-stop":
        return hook_stop()
    if name == "hook-pre-tool":
        return hook_pre_tool()
    if name not in CHECKS:
        print(f"ERROR unknown check {name}")
        return 2
    try:
        CHECKS[name](rest)
    except Fail as e:
        print(f"ERROR {e}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
