
The configuration file is given to the autoscaler binary via the `-c` or `--config` option:

```
./gh-actions-scaler -c autoscaler.yaml
```

If not specified, it will look for `~/.config/gh-actions-scaler/config.yaml`.

The following is an example configuration with dynamic machine provisioning disabled:

```yaml
log_level: info # Default: info

github:
  personal_access_token: "${GITHUB_ACCESS_TOKEN}"
  # or
  # personal_access_token: "${file:github_access_token.txt}"
  runners:
    name_prefix: "acme-{machine_id}-" # Default: "{machine_id}-"
    scope: "repo" # "repo" Default: "repo"
    repo_url: "https://github.com/foo/bar" # Required if scope == "repo"

machine_defaults: # Optional
  ssh:
    port: 8022
    username: "runner"
    ...
  runners:
    min_runners: 2 # default: 1
    max_runners: 4 # default: 1
    idle_timeout: 1m # default: 5m ...
  resources:
    ...

machines:
  - id: machine-1
    ssh:
      host: 172.18.0.100
      port: 8022 # Default: 22
      fingerprint: "..." # Optional
      username: "..."
      password: "..."
      # or
      private_key: "..."
      private_key_passphrase: "..."
      public_key: "..."
    runners:
      min_runners: 2 # Default: 1
      max_runners: 4 # Default: 1
      idle_timeout: 1m # Default: 5m (e.g. 60s, 7d)
    resources:
      # TODO: Something similar to https://docs.docker.com/compose/compose-file/deploy/#resources
      limits:
        cpus: ...
        memory: ...
      reservations:
        cpus: ...
        memoty: ...

  - id: machine-2
    # ...
```





