"""APT sources manager for SigmaOS-packages-kit / apturl / daemon-apt
(mintsources, repolib, apturl, apdaemon).

Principles: parse-then-validate (never string-append blindly), sources files
are treated as data, enable/disable is a comment toggle (idempotent), apturl
handling is a pure parser with a strict allowlist of parameters.
"""
import argparse, re, sys
from urllib.parse import urlparse, parse_qs

ENGINE = {
    "name": "sigma-sources",
    "summary": "Validate and manage APT sources + parse apt:// URIs",
    "language": "python",
    "verbs": ["list", "add", "remove", "enable", "test", "parse-apturl"],
    "principles": ["Fail-Fast", "Defensive Programming", "Least Astonishment",
                   "KISS", "Separation of Concerns", "Deterministic Testing"],
}

cli_doc = """\
$ sigma-sources list --file sources.list.d/ppa.list
$ sigma-sources add --file repos.list --url https://ppa.launchpadcontent.net/x/y/ubuntu --suite noble --components main
$ sigma-sources remove --file repos.list --match ppa.launchpadcontent.net/x/y
$ sigma-sources enable --file repos.list --match example.com --off
$ sigma-sources test --line "deb [signed-by=/k.gpg] https://example.com/apt noble main"
$ sigma-sources parse-apturl "apt://htop?architecture=amd64"
"""

VALID_COMPONENTS = {"main", "contrib", "non-free", "non-free-firmware", "restricted"}
ALLOWED_APTURL_PARAMS = {"architecture", "origin", "version", "selected"}
_SCHEMES = ("apt", "apturl")
_OPT_RE = re.compile(r"^\[(.*?)\]\s*")
_URL_PREFIXES = ("http://", "https://", "ftp://", "file:")


class SourcesError(Exception):
    pass


def _parse_entry(stripped):
    """Parse one uncommented sources line body into an entry dict."""
    kind, _, rest = stripped.partition(" ")
    rest = rest.strip()
    if kind not in ("deb", "deb-src"):
        raise SourcesError(f"unknown entry type {kind!r}")
    options = []
    m = _OPT_RE.match(rest)
    if m:
        options = m.group(1).split()
        rest = rest[m.end():].strip()
    parts = rest.split()
    if len(parts) < 2:
        raise SourcesError(f"too few fields: {stripped!r}")
    url, suite = parts[0], parts[1]
    components = parts[2:] or ["main"]
    if not url.startswith(_URL_PREFIXES):
        raise SourcesError(f"unsupported URL scheme: {url!r}")
    return {"type": kind, "url": url, "suite": suite, "components": components,
            "options": options, "enabled": True, "raw": stripped}


def parse_line(line):
    """Pure parser for one sources line -> dict, or None for blank/comment.

    A commented line that still parses as a valid source (e.g. '# deb ...')
    yields the entry with enabled=False; anything else is a plain comment.
    """
    stripped = line.strip()
    if not stripped:
        return None
    enabled = True
    if stripped.startswith("#"):
        enabled = False
        stripped = stripped.lstrip("#").strip()
        if not stripped:
            return None
    try:
        entry = _parse_entry(stripped)
    except SourcesError:
        if not enabled:
            return None  # plain human comment
        raise
    entry["enabled"] = enabled
    return entry


def format_line(entry):
    """Pure inverse of parse_line: dict -> canonical (uncommented) line."""
    opts = f"[{' '.join(entry['options'])}] " if entry.get("options") else ""
    comps = " ".join(entry["components"]) or "main"
    return f"{entry['type']} {opts}{entry['url']} {entry['suite']} {comps}"


def add(text, url, suite, components, kind="deb", options=()):
    """Pure: validate + append a new source line; fail-fast on duplicates."""
    bad = [c for c in components if c not in VALID_COMPONENTS]
    if bad:
        raise SourcesError(f"invalid component(s): {', '.join(bad)} "
                           f"(valid: {', '.join(sorted(VALID_COMPONENTS))})")
    entry = _parse_entry(format_line(
        {"type": kind, "url": url, "suite": suite,
         "components": list(components), "options": list(options)}))
    for line in text.splitlines():
        existing = parse_line(line)
        if existing and existing["url"] == url and existing["suite"] == suite:
            raise SourcesError(f"duplicate source: {url} {suite}")
    newline = format_line(entry)
    if text and not text.endswith("\n"):
        text += "\n"
    return text + newline + "\n"


def remove(text, match):
    """Pure: drop lines whose url or suite matches; returns (text, removed)."""
    kept, removed = [], []
    for line in text.splitlines():
        entry = parse_line(line)  # plain comments -> None -> kept verbatim
        if entry and (match in entry["url"] or match == entry["suite"]):
            removed.append(format_line(entry))
            continue
        kept.append(line)
    return "\n".join(kept) + ("\n" if kept else ""), removed


def set_enabled(text, match, enabled):
    """Pure: toggle the comment prefix on matching lines (idempotent)."""
    out, touched = [], 0
    for line in text.splitlines():
        entry = parse_line(line)
        if entry and (match in entry["url"] or match == entry["suite"]):
            if entry["enabled"] != enabled:
                touched += 1
            formatted = format_line(entry)
            out.append(formatted if enabled else "# " + formatted)
        else:
            out.append(line)
    return "\n".join(out) + ("\n" if out else ""), touched


def list_entries(text):
    """Pure: parse every line -> (entry_or_None, lineno, error_or_None)."""
    rows = []
    for i, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        try:
            rows.append((parse_line(line), i, None))
        except SourcesError as e:
            rows.append((None, i, str(e)))
    return rows


def validate_line(line):
    """Pure: full validation with a list of problems (empty == valid)."""
    problems = []
    try:
        entry = parse_line(line)
    except SourcesError as e:
        return [str(e)]
    if entry is None:
        return ["line is blank or a plain comment"]
    bad = [c for c in entry["components"] if c not in VALID_COMPONENTS]
    if bad:
        problems.append("unknown component(s): " + ", ".join(bad))
    if entry["suite"] in ("stable", "oldstable", "testing", "unstable", "sid"):
        problems.append(f"suite {entry['suite']!r} is a moving target; pin a codename")
    for opt in entry["options"]:
        if "=" not in opt:
            problems.append(f"malformed option: {opt!r}")
    return problems


def parse_apturl(uri):
    """Pure parser for apt:// / apturl:// URIs (apturl-style)."""
    parsed = urlparse(uri)
    if parsed.scheme not in _SCHEMES:
        raise SourcesError(f"not an apt URI: {uri!r}")
    # 'apt://pkg' -> pkg is the host; 'apt://host/path,pkg' -> packages in path
    raw_path = parsed.path.lstrip("/")
    if not raw_path and parsed.netloc:
        raw_path = parsed.netloc
    packages = [p for p in raw_path.split(",") if p]
    if not packages:
        raise SourcesError("apt URI names no packages")
    params = {}
    for key, values in parse_qs(parsed.query).items():
        if key not in ALLOWED_APTURL_PARAMS:
            raise SourcesError(f"disallowed apt URI param: {key!r}")
        params[key] = values[0]
    return {"packages": packages, "params": params}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog=ENGINE["name"], description=ENGINE["summary"])
    sub = p.add_subparsers(dest="cmd", required=True)
    ls = sub.add_parser("list", help="list parsed entries with errors")
    ls.add_argument("--file", required=True)
    ad = sub.add_parser("add", help="validate + append a source line")
    ad.add_argument("--file", required=True)
    ad.add_argument("--url", required=True)
    ad.add_argument("--suite", required=True)
    ad.add_argument("--components", nargs="+", default=["main"])
    ad.add_argument("--type", default="deb", choices=["deb", "deb-src"])
    rm = sub.add_parser("remove", help="remove lines matching a substring")
    rm.add_argument("--file", required=True)
    rm.add_argument("--match", required=True)
    en = sub.add_parser("enable", help="enable/disable matching lines")
    en.add_argument("--file", required=True)
    en.add_argument("--match", required=True)
    en.add_argument("--off", action="store_true", help="disable instead")
    ts = sub.add_parser("test", help="validate a single sources line")
    ts.add_argument("--line", required=True)
    pa = sub.add_parser("parse-apturl", help="parse an apt:// URI")
    pa.add_argument("uri")
    return p


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.cmd == "list":
            with open(args.file) as fh:
                rows = list_entries(fh.read())
            rc = 0
            for entry, lineno, err in rows:
                if err:
                    print(f"line {lineno}: ERROR {err}")
                    rc = 1
                elif entry:
                    state = "on " if entry["enabled"] else "off"
                    print(f"line {lineno}: [{state}] {entry['url']} "
                          f"{entry['suite']} {' '.join(entry['components'])}")
            return rc
        with open(args.file) as fh:
            text = fh.read()
        if args.cmd == "add":
            text = add(text, args.url, args.suite, args.components, kind=args.type)
        elif args.cmd == "remove":
            text, removed = remove(text, args.match)
            print(f"removed {len(removed)} line(s)")
        elif args.cmd == "enable":
            text, touched = set_enabled(text, args.match, not args.off)
            print(f"{'enabled' if not args.off else 'disabled'} {touched} line(s)")
        with open(args.file, "w") as fh:
            fh.write(text)
        return 0
    except (SourcesError, OSError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


def self_test() -> None:
    sample = """\
# SigmaOS main repo
deb https://archive.sigmaos.example noble main restricted
deb [arch=amd64 signed-by=/usr/share/keyrings/ppa.gpg] https://ppa.launchpadcontent.net/deadsnakes/ppa/ubuntu noble main
# deb-src https://archive.sigmaos.example noble main
deb https://mirror.example.org/debian bookworm contrib non-free
deb
"""
    rows = list_entries(sample)
    parsed = [(e, ln) for e, ln, err in rows if e]
    errors = [err for _, _, err in rows if err]
    assert len(parsed) == 4, parsed
    assert len(errors) == 1 and "too few fields" in errors[0], errors

    ppa = [e for e, _ in parsed if "ppa.launchpadcontent" in e["url"]][0]
    assert ppa["suite"] == "noble"
    assert ppa["options"] == ["arch=amd64", "signed-by=/usr/share/keyrings/ppa.gpg"]
    assert ppa["components"] == ["main"]
    assert ppa["enabled"] is True

    archive = [e for e, _ in parsed if "archive.sigmaos" in e["url"]]
    assert len(archive) == 2
    assert {e["type"] for e in archive} == {"deb", "deb-src"}
    assert {e["enabled"] for e in archive} == {True, False}

    text = "deb https://a.example noble main\n"
    new = add(text, "https://b.example", "noble", ["main", "contrib"])
    assert new.count("\n") == 2
    assert "b.example noble main contrib" in new
    # options round-trip through add
    with_opts = add(text, "https://c.example", "noble", ["main"],
                    options=["arch=amd64", "signed-by=/k.gpg"])
    assert "[arch=amd64 signed-by=/k.gpg] https://c.example" in with_opts
    assert parse_line(with_opts.splitlines()[1])["options"] == \
        ["arch=amd64", "signed-by=/k.gpg"]
    try:
        add(new, "https://a.example", "noble", ["main"])  # duplicate
    except SourcesError as e:
        assert "duplicate" in str(e)
    else:
        raise AssertionError("duplicate source not caught")
    try:
        add(text, "https://c.example", "noble", ["banana"])
    except SourcesError as e:
        assert "component" in str(e)
    else:
        raise AssertionError("bad component not caught")

    removed_text, removed = remove(new, "b.example")
    assert removed == ["deb https://b.example noble main contrib"]
    assert "b.example" not in removed_text

    toggled, touched = set_enabled(new, "a.example", False)
    assert touched == 1 and toggled.startswith("# deb https://a.example")
    toggled2, touched2 = set_enabled(toggled, "a.example", False)
    assert touched2 == 0 and toggled2 == toggled  # idempotent
    back, touched3 = set_enabled(toggled, "a.example", True)
    assert touched3 == 1 and not back.startswith("#")  # re-enable works

    assert validate_line("deb https://ok.example noble main") == []
    probs = validate_line("deb https://ok.example unstable main")
    assert any("moving target" in p for p in probs)
    assert validate_line("ftp://x") == ["unknown entry type 'ftp://x'"]
    assert any("component" in p
               for p in validate_line("deb https://x.example noble main banana"))
    assert any("option" in p
               for p in validate_line("deb [arch] https://x.example noble main"))

    info = parse_apturl("apt://htop,vim?architecture=amd64&origin=ppa:deadsnakes/ppa")
    assert info["packages"] == ["htop", "vim"]
    assert info["params"]["architecture"] == "amd64"
    assert parse_apturl("apturl://git")["packages"] == ["git"]
    try:
        parse_apturl("apt://pkg?evil=1")
    except SourcesError as e:
        assert "disallowed" in str(e)
    else:
        raise AssertionError("bad apturl param not caught")
    try:
        parse_apturl("apt://")
    except SourcesError:
        pass
    else:
        raise AssertionError("empty apturl not caught")
    try:
        parse_apturl("https://example.com")
    except SourcesError:
        pass
    else:
        raise AssertionError("non-apt scheme not caught")

    # round-trip: format(parse(x)) parses to the same dict
    line = "deb [arch=amd64] https://r.example noble main non-free"
    entry = parse_line(line)
    assert parse_line(format_line(entry)) == entry


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
