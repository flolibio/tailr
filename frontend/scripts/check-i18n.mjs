#!/usr/bin/env node
// i18n key 完整性检查
//
// 背景:@intlify/unplugin-vue-i18n 的 HMR 对 JSON 改动不可靠——新增 key 后
// dev server 经常不刷新消息表,运行时 t() 返回 key 原文(如显示
// "settings.updateDetected" 而非翻译)。此问题反复出现(invalidToken、
// systemFontLabel、updateDetected 等)。
//
// 本脚本扫描代码中所有 t('...') 引用,对比 locale JSON 的 key,
// 报告:① 代码引用了但 JSON 没有的 key(会导致显示原文)
//       ② 两个语言文件之间不一致的 key
//
// 用法:node scripts/check-i18n.mjs
// 建议在加完 i18n key 后、提交前跑一次。

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')
const SRC = resolve(ROOT, 'src')
const LOCALES_DIR = resolve(ROOT, 'src', 'locales')

// ── 收集所有源文件 ──
function walk(dir, files = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      walk(full, files)
    } else if (/\.(vue|ts|tsx)$/.test(entry)) {
      files.push(full)
    }
  }
  return files
}

// ── 提取代码中的 t('...') / t("...") 引用 ──
// 匹配 t('key')、t("key")、t('key', {...})，以及 $t('key') 形式
const KEY_RE = /\bt\(\s*['"`]([^'"`]+)['"`]/g
function extractKeys(content) {
  const keys = new Set()
  let m
  while ((m = KEY_RE.exec(content)) !== null) {
    keys.add(m[1])
  }
  return keys
}

// ── 把扁平 key 收集成集合(支持嵌套 JSON) ──
function flattenKeys(obj, prefix = '') {
  const keys = new Set()
  for (const [k, v] of Object.entries(obj)) {
    const full = prefix ? `${prefix}.${k}` : k
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      for (const sub of flattenKeys(v, full)) keys.add(sub)
    } else {
      keys.add(full)
    }
  }
  return keys
}

// ── 主流程 ──
const sourceFiles = walk(SRC)
const codeKeys = new Set()
for (const f of sourceFiles) {
  const content = readFileSync(f, 'utf8')
  for (const k of extractKeys(content)) codeKeys.add(k)
}

// 加载语言文件
const localeFiles = readdirSync(LOCALES_DIR).filter((f) => f.endsWith('.json'))
const locales = {}
for (const f of localeFiles) {
  const data = JSON.parse(readFileSync(join(LOCALES_DIR, f), 'utf8'))
  locales[f] = flattenKeys(data)
}

let hasError = false

// ① 代码引用了但某语言文件缺失的 key
for (const [file, keys] of Object.entries(locales)) {
  const missing = [...codeKeys].filter((k) => !keys.has(k))
  if (missing.length > 0) {
    hasError = true
    console.error(`\n❌ ${file}: 代码引用了但 JSON 缺失的 key(${missing.length} 个):`)
    for (const k of missing.sort()) console.error(`   - ${k}`)
  }
}

// ② 两个语言文件之间 key 不一致
const localeNames = Object.keys(locales)
if (localeNames.length >= 2) {
  const base = localeNames[0]
  const baseKeys = locales[base]
  for (let i = 1; i < localeNames.length; i++) {
    const other = localeNames[i]
    const otherKeys = locales[other]
    const onlyBase = [...baseKeys].filter((k) => !otherKeys.has(k))
    const onlyOther = [...otherKeys].filter((k) => !baseKeys.has(k))
    if (onlyBase.length > 0 || onlyOther.length > 0) {
      hasError = true
      console.error(`\n❌ ${base} 与 ${other} 的 key 不一致:`)
      if (onlyBase.length) {
        console.error(`   仅 ${base} 有:`)
        for (const k of onlyBase.sort()) console.error(`     - ${k}`)
      }
      if (onlyOther.length) {
        console.error(`   仅 ${other} 有:`)
        for (const k of onlyOther.sort()) console.error(`     - ${k}`)
      }
    }
  }
}

if (!hasError) {
  console.log(`✅ i18n key 检查通过(代码引用 ${codeKeys.size} 个 key,语言文件 ${localeNames.join('/')} 一致)`)
  process.exit(0)
} else {
  console.error('\n⚠️  如果刚加了 key 但 dev server 显示原文,这是 @intlify/unplugin-vue-i18n 的 HMR 失效问题,重启 dev server (npm run dev) 即可加载新 key。')
  process.exit(1)
}
