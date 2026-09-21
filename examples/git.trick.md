---
tags: [git, vcs]
---

## Show commit log

Compact one-line format with graph visualization.

```sh
git log --oneline --graph --all
```

## Interactive rebase

Rewrite the last N commits. Use this to squash, reorder, or edit commits.
Be careful with shared branches -- this rewrites history.

```sh
git rebase -i HEAD~<num_commits>
```

$ num_commits: echo -e "3\n5\n10"

## Stage changes interactively

Lets you select individual hunks to stage.

```sh
git add -p
```

## Stash changes

Save uncommitted changes to a stack without committing.

```sh
git stash push -m "<message>"
```

## Cherry-pick a commit

Apply a specific commit from another branch.

```sh
git cherry-pick <commit_hash>
```

## Show diff of staged changes

```sh
git diff --cached
```

## Reset to remote branch

Discard all local changes and reset to match the remote.
WARNING: This is destructive and cannot be undone.

```sh
git reset --hard origin/<branch>
```

$ branch: git branch --format '%(refname:short)'
