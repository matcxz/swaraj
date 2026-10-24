let:
  major: 0
  minor: 1
  version: "${major}.${minor}"
  enabled: true
  dependencies: ["gcc", "make"]
  allDependencies: dependencies ++ ["ncurses"]
  featureCount: 1 + 2 * 3
  useNativeBuild: enabled && featureCount > 2
  summary: if useNativeBuild then "Swaraj ${version}" else "Swaraj"
in:
package swaraj:
  version: version
  enabled: enabled
  dependencies: allDependencies
  featureCount: featureCount
  summary: summary
