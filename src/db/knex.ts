import { createRequire } from 'module'
import knex, { Knex } from 'knex'

const nodeRequire = createRequire(import.meta.url)
const config = nodeRequire('../../knexfile.cjs')

export const db: Knex = knex(config)

export async function closeDatabase(): Promise<void> {
  await db.destroy()
}
