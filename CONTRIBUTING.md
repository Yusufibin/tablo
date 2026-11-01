# Contribuer à Tablo

Les corrections, améliorations de l’interface et nouveaux cas de test sont les bienvenus.
Pour proposer une fonctionnalité importante, ouvre d’abord une issue avec le besoin
et un exemple d’utilisation.

## Préparer le projet

Installe Rust stable et les prérequis décrits dans le [README](README.md#installation),
puis clone le dépôt et lance un exemple :

```bash
git clone https://github.com/Yusufibin/tablo.git
cd tablo
cargo run --locked -- samples/people.csv
```

## Valider une modification

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Pour une correction de comportement, ajoute si possible un test de régression.
Utilise des données fictives dans les exemples et les rapports de bogue.

## Ouvrir une pull request

Décris le problème, le comportement obtenu et les vérifications effectuées.
Ajoute une capture d’écran lorsque la modification touche à l’interface.
Garde chaque pull request centrée sur un objectif.
