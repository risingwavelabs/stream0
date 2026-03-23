import { readFileSync } from 'node:fs'
import { join } from 'node:path'

type PackageJson = {
  name?: string
  version?: string
}

const packageJsonPath = join(__dirname, '..', '..', 'package.json')
const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8')) as PackageJson

export const APP_NAME = packageJson.name ?? 'box0-flow'
export const APP_VERSION = packageJson.version ?? '1.0.0'
