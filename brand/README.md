# Identité visuelle de Weft

Licence MIT, comme le reste du projet. Tout est en **SVG dessiné à la main**, sans aucune image générée par IA.

## Symbole
Trois gros pixels et une aiguille qui les traverse. Le quatrième pixel manque : c'est par là que l'aiguille sort. Weft coud ensemble les appareils, à travers le numérique.

## Couleurs
| Rôle | Valeur |
| --- | --- |
| Orange « coucher de soleil » (symbole) | `#F26A21` |
| Encre (fond sombre, texte sur clair) | `#17130F` |
| Crème (fond clair, texte sur sombre) | `#F6EFE6` |

Deux couleurs par usage : orange + encre, ou orange + crème. Pas de dégradé, pas d'ombre, pas d'effet.

## Fichiers (`final/`)
| Fichier | Usage |
| --- | --- |
| `weft-mark.svg` | Symbole seul en `currentColor`, pour l'intégrer dans les apps |
| `weft-icon-dark.svg`, `weft-icon-light.svg` | Icône d'app carrée à fond plein. Chaque plateforme applique sa forme : squircle macOS, cercle ou forme adaptative Android, carré arrondi Windows |
| `weft-favicon.svg` | Favicon, qui passe en sombre si le navigateur l'est |
| `weft-lockup.svg` | Symbole + mot « weft » en `currentColor` |
| `weft-lockup-dark.svg`, `weft-lockup-light.svg` | Symbole + mot sur fond sombre ou clair (README, site) |

## Règles
- Marge libre autour du symbole : au moins la largeur d'un pixel (28 % du symbole).
- Taille minimale : 16 px pour l'icône, 80 px de large pour le symbole + mot.
- Ne pas recolorer l'aiguille ou les pixels séparément, ne pas tourner le symbole, ne pas ajouter de contour.
- Le mot « weft » s'écrit en minuscules dans le logo. Il est dessiné au trait, sans police : ne pas le retaper dans une police.
- Les PNG, `.icns`, `.ico` et icônes adaptatives Android se génèrent depuis ces SVG au moment du build. On ne les retouche pas à la main.

## Pistes écartées
`variants/` (6 premières pistes) et `variants-2/` (dérivés de « Pixels cousus »). La version retenue est la 2B, « Gros pixels ».
