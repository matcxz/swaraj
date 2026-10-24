let:
  base: builtins.import "./imported-base.sw"
  appDependencies: base.dependencies ++ ["openssl"]
in:
package example-app:
  basePackage: base.packageName
  version: base.version
  dependencies: appDependencies
  sourceAvailable: builtins.pathExists "./imported-base.sw"
