let:
  major: 0
  minor: 1
  increment: number: number + 1
  nextMinor: increment minor
  version: "${major}.${nextMinor}"
  enabled: true
  dependencies: ["gcc", "make"]
  allDependencies: dependencies ++ ["ncurses"]
  featureCount: 1 + 2 * 3
  useNativeBuild: enabled && featureCount > 2
  summary: if useNativeBuild then "Swaraj ${version}" else "Swaraj"
  metadata: {
    name = "Swaraj";
    version = version;
  }
  packageMetadata: metadata // { implementation = "Rust"; }
  recursiveMetadata: rec {
    major = 0;
    minor = 2;
    version = "${major}.${minor}";
    description = "Swaraj ${version}";
  }
  dependencySummary: builtins.concatStringsSep ", " allDependencies
in:
package swaraj:
  version: version
  enabled: enabled
  dependencies: allDependencies
  dependencyCount: builtins.length allDependencies
  firstDependency: builtins.head allDependencies
  remainingDependencies: builtins.tail allDependencies
  metadataKeys: builtins.attrNames packageMetadata
  buildNumber: builtins.toString featureCount
  dependencySummary: dependencySummary
  featureCount: featureCount
  summary: summary
  metadata: packageMetadata
  recursiveMetadata: recursiveMetadata
  metadataName: packageMetadata.name
  hasVersion: packageMetadata ? version
  channel: packageMetadata.channel or "stable"
