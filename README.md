# Steel

Steel est la couche de configuration declarative du build. Il lit un `steelconf`, verifie la coherence (workspace, profils, toolchains, targets) et produit un fichier stable `steel.log`. Vitte utilise ensuite `steel.log` pour executer un build deterministe.

## En bref
- Declarer ce qu on veut construire, de facon claire et reproductible.
- Separer la configuration (Steel) de l execution (Vitte).
- Utiliser `steel.log` comme contrat stable entre les deux.

## Demarrage
```text
steel run steelconf
steel build steelconf
```

## Fichiers
- `steelconf` : configuration principale
- `steel.log` : configuration gelee et normalisee (utilisee par Vitte)

## Editeur integre (steecleditor)
1) Editeur terminal pour modifier `steelconf`.
2) Autocompletion, indentation, recherche.
3) Onglets, diff, themes.
4) Support Rust (coloration et confort d edition de base).

## Voir aussi
- `doc/manifest.md`
