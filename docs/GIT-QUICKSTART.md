# Git for this project

Git records checkpoints locally; GitHub shares those checkpoints with other people.
The `main` branch is the first line of development. A commit describes a meaningful
change, such as “Add reviewed project intake and native workshop”.

Useful read-only commands:

```sh
git status                 # What changed since the last checkpoint?
git diff                   # Inspect changes before recording them
git log --oneline           # See the recorded checkpoints
```

Configure your preferred Git author name and email before recording history. Git
author emails are included in commit metadata; GitHub offers private commit
addresses for contributors who prefer them. Confirm your public identity before
publishing. Personal account settings stay in local Git configuration, not here.

Creating a commit does not upload anything. Creating a public GitHub repository,
pushing the code and publishing a release are separate steps. The source repository
is https://github.com/CodeFu72/launchshelf. A packaged release and the finished demo
remain separate milestones.
