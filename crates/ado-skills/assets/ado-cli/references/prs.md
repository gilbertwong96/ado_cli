# Pull Requests

```bash
# List PRs in a repo (table: ID, Title, Source, Target, Status)
ado prs list MyProject MyRepo --status active --creator "alice@example.com"

# Show details
ado prs show MyProject MyRepo 42

# Create
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main --draft

# View diff (3 modes)
ado prs diff MyProject MyRepo 42                                    # file list with +/- counts
ado prs diff MyProject MyRepo 42 --file src/app.ex                   # per-file unified diff
ado prs diff MyProject MyRepo 42 --unified | delta                  # full unified diff stream

# Diff handles new, edited, and deleted files correctly:
ado prs diff MyProject MyRepo 42 --file new_file.ex                  # new file: shows all + lines
ado prs diff MyProject MyRepo 42 --file deleted.ex                   # deleted: shows all - lines

# Review and merge
ado prs approve MyProject MyRepo 42                                 # vote +10
ado prs complete MyProject MyRepo 42 --merge-strategy squash --delete-source
ado prs abandon MyProject MyRepo 42

# Comments (multi-word content does NOT need quoting)
ado prs comments add MyProject MyRepo 42 --content LGTM ship it
ado prs comments add MyProject MyRepo 42 --content @notes.md        # from file
echo "review" | ado prs comments add MyProject MyRepo 42 --content - # from stdin
ado prs comments add MyProject MyRepo 42 --file-path src/app.ex --line 10 --content nitpick
ado prs comments add MyProject MyRepo 42 --file-path src/app.ex --line 10 --end-line 20 --content codeblock review
ado prs comments list MyProject MyRepo 42 --all                     # full content view
ado prs comments update MyProject MyRepo 42 THREAD_ID COMMENT_ID --content "updated text"
ado prs comments update MyProject MyRepo 42 THREAD_ID COMMENT_ID --status closed --resolved-by-me
ado prs comments delete MyProject MyRepo 42 THREAD_ID               # close a thread
ado prs comments delete MyProject MyRepo 42 THREAD_ID --comment-id 1 # delete a comment
ado prs comments resolve MyProject MyRepo 42 THREAD_ID              # resolve as fixed
ado prs comments resolve MyProject MyRepo 42 THREAD_ID --resolved-by-me --status wontFix

# Reviewers
ado prs reviewers list MyProject MyRepo 42
ado prs reviewers list MyProject MyRepo 42 --search alice   # fuzzy filter by name/email
ado prs reviewers add MyProject MyRepo 42 --reviewer USER_GUID
ado prs reviewers remove MyProject MyRepo 42 --reviewer USER_GUID
```

