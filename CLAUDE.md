# CLAUDE.md

## Branch policy

- **Exception (approved 2026-09-18):** until CI/CD workflows are set up, working and committing directly on `main` is allowed.
- Even under this exception, quality gates are mandatory:
  - Implementation (worktree-code): run `review-all` in review mode (`jobs=review`).
  - Before every commit: run `review-all` in doc mode (`jobs=docs`).
- Once CI/CD exists, this exception is revoked: all changes must go through PRs.

## Language

- Repo language is English (see `.language`): issues, PRs, and commit messages in English.
