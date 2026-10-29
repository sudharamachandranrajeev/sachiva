// Screenshot harness: fakes the Rust side with demo.json (from core/examples/demo.rs).
// Synthetic data only. Hash options: view, edit, capture, theme, platform, mode
// (ok, locked, first, setup), update (an update is waiting), kind (install kind).
import { mockIPC } from '@tauri-apps/api/mocks'
import demo from './demo.json'

const p = new URLSearchParams(location.hash.slice(1))
if (p.get('theme')) localStorage.setItem('sachiva.theme', p.get('theme')!)
const mode = p.get('mode') ?? 'ok'

mockIPC((cmd, args: any) => {
  switch (cmd) {
    case 'status':
      return { unlocked: mode !== 'locked', problem: mode === 'locked' ? 'The stored database key cannot be read.' : null,
        recoveryPending: mode === 'first', keyStorage: 'keyring', platform: p.get('platform') ?? 'linux',
        notice: null, setupDone: mode !== 'setup' }
    case 'recovery_key': return '3f9a-0c21-7be4-91d0-55aa-e2c7-0f13-8b6d-c4e9-27f0-a1b3-6d58-9e02-f7c4-3a16-b8d5'
    case 'get_dashboard': return demo.dashboard
    case 'done_tasks': return demo.done
    case 'trash': return []
    case 'get_settings': return demo.settings
    case 'backups': return { folder: '/home/user/.local/share/dev.sachiva.sachiva/backups', files: [
      { name: 'sachiva-2026-10-29.db', kind: 'daily', bytes: 98304 },
      { name: 'pre-update-2026-10-28-171502-0.1.0.db', kind: 'preUpdate', bytes: 94208 },
      { name: 'sachiva-2026-10-28.db', kind: 'daily', bytes: 94208 },
    ] }
    case 'update_status': return { kind: p.get('kind') ?? 'appImage', canInstall: !['portable', 'other'].includes(p.get('kind') ?? ''),
      current: '0.1.0', previous: null, mode: 'ask' }
    case 'check_update': return p.get('update') ? { version: '0.1.1', notes: 'Fixes a reminder that came twice.\nFaster start.',
      date: null, canInstall: !['portable', 'other'].includes(p.get('kind') ?? '') } : null
    case 'rollback_plan': return null
    case 'autostart': return true
    case 'alarm_access': return { exact: false, batteryUnrestricted: false }
    case 'check_text': {
      const out = []
      for (const m of String(args.text).matchAll(/\b\d{3} \d{3} \d{4}\b/g))
        out.push({ kind: 'healthNumber', start: m.index, end: m.index! + m[0].length, text: m[0], message: 'This looks like an NHS or H&C number. Use your reference code instead.' })
      return out
    }
    case 'new_task': return { ...demo.dashboard.items[0].task, id: crypto.randomUUID(), title: args.title }
    case 'plugin:app|version': return '0.1.0'
    case 'plugin:event|listen': return 1
    default: return null
  }
})

await import('../src/main.ts')
const { app } = await import('../src/lib/state.svelte.ts')
await new Promise((r) => setTimeout(r, 300))
if (p.get('view')) app.view = p.get('view') as any
if (p.get('edit')) {
  const t = structuredClone(demo.dashboard.items[0].task)
  t.notes = 'Offer to next on list. Was 943 476 5919 on the old sheet.'
  app.editing = { task: t as any, isNew: false }
}
if (p.get('capture')) app.capturing = true
