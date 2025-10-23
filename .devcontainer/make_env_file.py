#!/usr/bin/env python3
#
# assumes git worktree was made using relative paths e.g.
# git worktree add ../dir-name branch-name --relative-paths
import sys
from pathlib import Path
from textwrap import dedent
import subprocess
from os.path import relpath


def make_config(local_workspace_path: Path):
    env_workspace_path = Path(f"/workspaces/{local_workspace_path.name}")
    local_git_path = Path(
        subprocess.run(
            "git rev-parse --path-format=absolute --git-common-dir".split(" "),
            capture_output=True,
            text=True,
        ).stdout.strip()
    )
    env_git_path = (
        env_workspace_path / ".git"
        if local_workspace_path / ".git" == local_git_path
        else (
            env_workspace_path
            / relpath(local_git_path.parent, local_workspace_path)
            / ".git"
        ).resolve()
    )

    return dedent(
        f"""
        LOCAL_WORKSPACE_PATH={local_workspace_path}
        ENV_WORKSPACE_PATH={env_workspace_path}
        LOCAL_GIT_PATH={local_git_path}
        ENV_GIT_PATH={env_git_path}
        """
    ).strip()


if __name__ == "__main__":
    local_workspace_path = Path(sys.argv[1])

    with open(Path(__file__).absolute().parent / f"{sys.argv[2]}.env", "w") as f:
        f.write(make_config(local_workspace_path))
