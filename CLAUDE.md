# CLAUDE.md

## Branch policy

- All changes go through PRs. Direct commits to `main` are prohibited (branch protection enforced, admins included).
- Merging requires the `test` CI check to pass; branches must be up to date with `main` (strict) and use linear history (squash or rebase merge).
- No review approvals required (solo project); merge after CI passes.
- `--admin` merge cannot bypass protection here. If ever needed, temporarily change protection settings.
- Quality gates are mandatory:
  - Implementation (worktree-code): run `review-all` in review mode (`jobs=review`).
  - Before every commit: run `review-all` in doc mode (`jobs=docs`).

## Language

- Repo language is English (see `.language`): issues, PRs, and commit messages in English.
