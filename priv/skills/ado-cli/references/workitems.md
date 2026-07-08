# Work Items


```bash
# List work items (table: ID, Title, Type, State, Assigned To)
ado workitems list MyProject --state Active --type Bug --top 20

# Show details
ado workitems show 42

# WIQL query
ado workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'"

# Create
ado workitems create MyProject --type Bug --title "Login fails" --description "Steps to reproduce: ..." --tags "ui,critical" --priority 1

# Update state or fields
ado workitems update 42 --state Resolved --assigned_to "Jane Smith"

# Delete
ado workitems delete 42
```

