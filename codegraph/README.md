# Click paths of OpenProject, read from its Rails source

These files say what OpenProject lets a user reach and who may reach it, read from the
Rails source by `rails-codegraph` (in AdaWorldAPI/zammad-rs). Nothing here is
written by hand. The configuration is `openproject.toml`. No code in the tool names
OpenProject.

| File | What it holds |
| --- | --- |
| `harvest/klickwege-openproject.json` | Per stratum: declarations read, how many became rows, how many escaped |
| `harvest/access-openproject.json` | Controller filters (`before_action` and the like) with their `only:` and `except:` lists |
| `harvest/frontend-routes-openproject.json` | Frontend routes (empty: none are configured) |
| `harvest/seed-rows-openproject.json` | Seed rows (empty: none are configured) |
| `harvest/class-macros-contracts-openproject.json` | Every `attribute`, `validate`, `validates` and `attribute_alias` call in the contracts, with its `permission:` and `writable:` arguments. Contracts are where OpenProject validates and authorizes writes: its create services save with `save(validate: false)` |
| `harvest/constants-openproject.json` | Literal class constants (empty: none are configured) |
| `harvest/slag-klickwege-openproject.json` | Everything that was not read, each with its reason and remedy: the work queue |

Each stratum balances: declarations = rows + escaped.

## Regenerate

From a zammad-rs checkout, with the OpenProject source checked out:

```bash
OPENPROJECT_SRC=/path/to/openproject cargo run -p rails-codegraph --bin rails-codegraph -- /path/to/this/repo/codegraph/openproject.toml --klickwege
```

The tool writes into `harvest/` beside the configuration.
