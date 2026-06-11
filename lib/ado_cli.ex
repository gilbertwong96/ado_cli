defmodule AdoCli do
  @moduledoc """
  Azure DevOps CLI — a command-line tool for managing Azure DevOps.

  ## Features

    * **Projects** — list, show, create, update, delete
    * **Repositories** — list, show, create, delete, list branches
    * **Work Items** — list, show, WIQL query, create, update
    * **Pipelines** — list, show, run (trigger builds)
    * **Pull Requests** — list, show, create, complete (merge), abandon
    * **Releases** — list, show

  ## Authentication

  Multiple auth methods, auto-resolved in priority order:

    1. CLI flags `--org` / `--pat`
    2. Environment variables `ADO_ORG` / `ADO_PAT`
    3. Azure CLI token (`az login`)
    4. Persistent config file (`ado_cli login`)

  Default login opens a browser for OAuth sign-in. For MSA personal orgs
  (`*.visualstudio.com`), API calls delegate to `az devops invoke` for
  reliable cross-tenant authentication.

  See [AUTH.md](AUTH.md) for the full authentication guide.

  ## Quick Start

      export ADO_ORG=myorg ADO_PAT=mytoken
      ado_cli projects list
      ado_cli repos list MyProject
      ado_cli workitems create MyProject --type Bug --title "Fix login"
      ado_cli prs create MyProject myrepo --title "New feature" --source dev --target main

  See [USAGE.md](USAGE.md) for the complete command reference.

  ## Build

      mix escript.build                    # Local escript
      MIX_ENV=prod mix release             # Cross-platform binary (Burrito)
  """
end
