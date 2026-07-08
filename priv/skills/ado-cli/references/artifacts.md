# Releases, Packages & Test Results

## Releases


```bash
ado releases list MyProject --definition_id 5 --status active
ado releases show MyProject 42
```


## Packages (Universal)


```bash
ado packages list MyProject MyFeed
ado packages versions MyProject MyFeed my-package
ado packages show MyProject MyFeed my-package 1.0.0
```


## Test Results and Coverage


```bash
ado test-results list MyProject
ado test-results show MyProject 42
ado test-results publish MyProject --name "CI Suite" --file coverage.cobertura.xml --build-id 99
ado test-coverage show MyProject 99
```

