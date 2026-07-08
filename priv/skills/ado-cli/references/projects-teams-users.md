# Projects, Teams, Users & Extensions

## Projects


```bash
# List all projects (table: Name, ID, State, Visibility)
ado projects list
ado projects list --state wellFormed --top 20

# Show a single project
ado projects show MyProject

# Create a project
ado projects create MyNewProject --description "My new project" --visibility private
# Visibility: private (default) or public. Process: Agile, Scrum, CMMI, Basic.

# Update (rename or change description)
ado projects update MyProject --name "Renamed Project"

# Delete (--force skips confirmation)
ado projects delete OldProject --force
```


## Teams


```bash
ado teams list MyProject
ado teams members list MyProject "My Team"
```


## Users (Entitlements)


```bash
ado users list
ado users show "alice@example.com"
ado users add --email "newuser@example.com" --license professional
ado users remove "user_id_or_email"
```


## Extensions


```bash
ado extensions list
ado extensions install ms.azure-devops-utilities --publisher ms
ado extensions uninstall "ms.azure-devops-utilities"
```

