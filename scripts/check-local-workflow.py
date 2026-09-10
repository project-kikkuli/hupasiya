#!/usr/bin/env python3
"""Agent-authored executable check using real hp/hn binaries and disposable Git repos.

No model, remote Git service, Docker daemon, or existing session is used.
"""

import argparse
import json
import shlex
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hp", type=Path, required=True)
    parser.add_argument("--hn", type=Path, required=True)
    args = parser.parse_args()
    hp, hn = args.hp.resolve(strict=True), args.hn.resolve(strict=True)

    with tempfile.TemporaryDirectory(prefix="hp workflow's ") as temporary:
        root = Path(temporary).resolve()
        repo = root / "example project"
        repo.mkdir()
        home = root / "home"
        home.mkdir()
        # Deliberately omit hn from PATH: the configured binary must be honored.
        env = {"HOME": str(home), "PATH": "/usr/bin:/bin", "GIT_CONFIG_NOSYSTEM": "1"}

        def run(command, cwd=repo, *, success=True, input=None):
            result = subprocess.run(
                [str(part) for part in command], cwd=cwd, env=env,
                input=input, capture_output=True, text=True, timeout=30,
            )
            assert (result.returncode == 0) == success, (
                f"{command!r}: exit {result.returncode}\n{result.stdout}\n{result.stderr}"
            )
            return result.stdout if success else result.stdout + result.stderr

        def git(*command, cwd=repo):
            return run(["git", *command], cwd)

        def sessions(cwd=repo):
            return {s["name"]: s for s in json.loads(run([hp, "list", "--all", "--format=json"], cwd))}

        git("init", "-b", "main")
        git("config", "user.name", "Synthetic Workflow Test")
        git("config", "user.email", "test@example.invalid")
        (repo / ".gitignore").write_text(".hn-state/\nstate/\n.hp/\n")
        (repo / ".hapusiyas.yml").write_text(
            f"hp:\n  hn:\n    command: {json.dumps(str(hn))}\n"
            "  sessions:\n    metadata_dir: state/sessions\n    context_dir: state/contexts\n"
            "  ai_tool:\n    context_strategy: env\n"
        )
        git("add", ".")
        git("commit", "-m", "Synthetic fixture")
        assert sessions() == {}

        run([hp, "new", "parent", "--from", "main"])
        parent = sessions()["parent"]
        parent_path = Path(parent["workbox_path"])
        context = Path(parent["context_dir"]) / "context.md"
        assert parent["base_branch"] == "main"
        assert parent_path.is_absolute() and context.is_absolute()
        assert context.is_relative_to(repo / "state/contexts") and context.is_file()
        context.write_text("Synthetic context that must survive switching.\n")
        nested = parent_path / "nested"
        nested.mkdir()
        assert sessions(nested)["parent"]["id"] == parent["id"]
        assert "Synthetic context" in run([hp, "context", "view", "parent"], nested)
        run([hp, "info", "parent"], nested)

        probe = (
            "import os; from pathlib import Path; "
            "assert Path.cwd() == Path(os.environ['HP_WORKBOX']); "
            "assert 'Synthetic context' in Path(os.environ['HP_CONTEXT']).read_text(); "
            "assert os.environ['HP_SESSION'] == 'parent'; print('consumer-ok')"
        )
        shell_setup = run([hp, "switch", "parent", "--output-shell"], nested)
        shell_probe = f"{shell_setup}\n{shlex.quote(sys.executable)} -c {shlex.quote(probe)}"
        assert "consumer-ok" in run(["/bin/sh", "-c", shell_probe], repo)
        assert "consumer-ok" in run([hp, "exec", "parent", "--", sys.executable, "-c", probe], nested)
        assert "consumer-ok" in run([hp, "launch", "parent", "--tool", sys.executable, "--", "-c", probe], nested)
        run([hp, "exec", "parent", "--", "/bin/sh", "-c", "exit 7"], nested, success=False)
        print("PASS: configured binary, shared metadata, persisted context, exec, launch, failure propagation")

        (parent_path / "parent.txt").write_text("parent contribution\n")
        git("add", "parent.txt", cwd=parent_path)
        git("commit", "-m", "Parent contribution", cwd=parent_path)
        run([hp, "new", "child", "--parent", "parent"], nested)
        child = sessions()["child"]
        child_path = Path(child["workbox_path"])
        assert child["base_branch"] == "parent"
        assert (child_path / "parent.txt").read_text() == "parent contribution\n"
        assert child["parent"] == "parent"
        assert "child" in sessions()["parent"]["children"]

        (child_path / "child.txt").write_text("child contribution\n")
        git("add", "child.txt", cwd=child_path)
        git("commit", "-m", "Child contribution", cwd=child_path)
        run([hp, "gather", "parent"], nested)
        assert (parent_path / "child.txt").read_text() == "child contribution\n"
        (parent_path / "later.txt").write_text("later contribution\n")
        git("add", "later.txt", cwd=parent_path)
        git("commit", "-m", "Later contribution", cwd=parent_path)
        run([hp, "cascade", "parent"], nested)
        assert (child_path / "later.txt").read_text() == "later contribution\n"
        print("PASS: child starts from parent; real Git gather/cascade exchange committed changes")

        # A failed merge must be a failing command, with conflicts left visible
        # for the developer to resolve; it must not be reported as successful.
        (parent_path / "later.txt").write_text("parent edit\n")
        git("add", "later.txt", cwd=parent_path)
        git("commit", "-m", "Parent diverges", cwd=parent_path)
        (child_path / "later.txt").write_text("child edit\n")
        git("add", "later.txt", cwd=child_path)
        git("commit", "-m", "Child diverges", cwd=child_path)
        assert "CONFLICT" in run([hp, "cascade", "parent"], nested, success=False)
        git("rev-parse", "--verify", "MERGE_HEAD", cwd=child_path)
        git("merge", "--abort", cwd=child_path)
        assert "CONFLICT" in run([hp, "gather", "parent"], nested, success=False)
        git("rev-parse", "--verify", "MERGE_HEAD", cwd=parent_path)
        git("merge", "--abort", cwd=parent_path)
        print("PASS: merge conflicts propagate failure and preserve resolvable Git state")

        run([hp, "new", "orphan", "--parent", "missing"], success=False)
        assert "orphan" not in sessions()
        assert not (root / "orphan").exists()
        dirty = child_path / "unsaved.txt"
        dirty.write_text("keep this work\n")
        run([hp, "close", "child", "--remove-workbox", "--archive"], success=False)
        assert dirty.read_text() == "keep this work\n"
        assert sessions()["child"]["status"] == "active"
        git("add", "unsaved.txt", cwd=child_path)
        git("commit", "-m", "Preserve work", cwd=child_path)
        run([hp, "close", "child", "--remove-workbox", "--archive"])
        assert not child_path.exists()
        assert sessions()["child"]["status"] == "archived"
        assert context.is_file()
        run([hp, "close", "parent", "--remove-workbox", "--archive"], input="y\n")
        assert not parent_path.exists()
        assert sessions()["parent"]["status"] == "archived"
        print("PASS: invalid-parent preflight, dirty-work protection, archive and cleanup")

        default_repo = root / "default project"
        default_repo.mkdir()
        env["PATH"] = f"{hn.parent}:/usr/bin:/bin"
        git("init", "-b", "main", cwd=default_repo)
        git("config", "user.name", "Synthetic Workflow Test", cwd=default_repo)
        git("config", "user.email", "test@example.invalid", cwd=default_repo)
        (default_repo / ".gitignore").write_text(".hp/\n.hn-state/\n.claude/commands/hp_context.md\n")
        git("add", ".gitignore", cwd=default_repo)
        git("commit", "-m", "Default fixture", cwd=default_repo)
        run([hp, "new", "plain"], default_repo)
        plain = sessions(default_repo)["plain"]
        plain_context = Path(plain["context_dir"]) / "context.md"
        assert plain_context.is_relative_to(default_repo / ".hp/contexts")
        default_probe = (
            "import os; from pathlib import Path; "
            "context = Path(os.environ['HP_CONTEXT']); assert context.is_file(); "
            "command = Path('.claude/commands/hp_context.md').read_text(); "
            "assert str(context) in command; print('default-launch-ok')"
        )
        assert "default-launch-ok" in run([hp, "launch", "plain", "--tool", sys.executable, "--", "-c", default_probe], default_repo)
        run([hp, "close", "plain", "--remove-workbox", "--archive"], default_repo)
        print("PASS: zero-config session and default slash-command context handoff")



if __name__ == "__main__":
    main()
