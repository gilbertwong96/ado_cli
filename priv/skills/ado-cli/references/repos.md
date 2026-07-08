# Repositories & Branch Policies

### Repositories

```bash
# List repos in a project
ado repos list MyProject

# Show a single repo
ado repos show MyProject MyRepo

# Create a repo
ado repos create MyProject --name "new-repo" --default_branch main

# List branches
ado repos branches MyProject MyRepo
ado repos branches MyProject MyRepo --filter feature

# Delete
ado repos delete MyProject MyRepo --force
```

### Branch policies

```bash
ado branch-policies list MyProject MyRepo
ado branch-policies show MyProject MyRepo POLICY_ID
ado branch-policies create MyProject MyRepo --type UUID --branch refs/heads/main --blocking
ado branch-policies update MyProject MyRepo POLICY_ID --enabled false
ado branch-policies delete MyProject MyRepo POLICY_ID
```

