# zer0fun-modules

Registre et modules fun pour [zer0gh0st](https://github.com/Richerrail/zer0gh0st).

## Modules disponibles

| Module | Version | Description |
|--------|---------|-------------|
| dosgames | 0.1.0 | Lanceur de jeux DOS via DOSBox (.IMA / .IMG / .EXE) |

## Installation d'un module

```bash
# Lister les modules disponibles
zer0build registry list --source https://raw.githubusercontent.com/Richerrail/zer0fun-modules/main/registry.json

# Installer un module
zer0build registry install dosgames --source https://raw.githubusercontent.com/Richerrail/zer0fun-modules/main/registry.json
```

## Publier un module

1. Créer le module avec `zer0build new <nom>`
2. Compiler : `zer0build build --release`
3. Publier dans le registre :
   ```bash
   zer0build registry publish --binary target/release/lib<module>.so \
     --registry registry.json \
     --url "https://github.com/Richerrail/zer0fun-modules/releases/download/v0.1.0/lib<module>.so"
   ```
4. Créer une GitHub Release avec le `.so`
5. Commit + push le `registry.json`

## Structure

```
zer0fun-modules/
├── registry.json          # Index des modules
├── modules/
│   └── dosgames/          # Source du module
│       ├── Cargo.toml
│       ├── zer0fun.toml
│       └── lib.rs
└── README.md
```

## Prérequis

- [zer0gh0st](https://github.com/Richerrail/zer0gh0st) installé (fournit `zer0fun-api` + signature)
- [zer0build](https://github.com/Richerrail/zer0build) pour compiler/publier/installer

## Licence

MIT
