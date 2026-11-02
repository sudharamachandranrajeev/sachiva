// Screenshot harness: fakes the Rust side with demo.json (from export/examples/demo.rs).
// Synthetic data only. Hash options: view, edit, capture, report (performance,
// waiting; with view=reports), theme, platform, mode
// (ok, locked, first, setup), update (an update is waiting), kind (install kind).
import { mockIPC } from '@tauri-apps/api/mocks'
import demo from './demo.json'

const p = new URLSearchParams(location.hash.slice(1))
if (p.get('theme')) localStorage.setItem('sachiva.theme', p.get('theme')!)
const mode = p.get('mode') ?? 'ok'
const clinicEntry = { id: '0192f0c4-0000-7000-8000-000000000011', listId: demo.waitlists[0].id,
  reference: 'REF-A17', target: '2026-11-05', appointmentType: 'Review', minutes: 30,
  days: [], earliest: null, latest: null, state: { state: 'waiting' }, revision: 1,
  createdAt: '2026-10-30T09:00:00Z', updatedAt: '2026-10-30T09:00:00Z' }
const clinicSlot = { id: '0192f0c4-0000-7000-8000-000000000012', listId: clinicEntry.listId,
  start: '2026-11-04T10:30:00Z', minutes: 30, appointmentType: 'Review', state: { state: 'open' },
  contacts: [], revision: 1, createdAt: '2026-10-30T09:00:00Z', updatedAt: '2026-10-30T09:00:00Z' }
const clinicSummary = { open: 1, filled: 0, lost: 0, refillPercent: null, meanFillMinutes: null }
const workJobs = [
  { ...demo.dashboard.items[0].task, id:'0192f0c4-0000-7000-8000-000000000021', title:'Clinic A morning session', kind:{type:'appointment'}, status:{state:'open'}, appointment:'2026-11-04T09:00:00Z', workflow:{type:'session',listId:clinicEntry.listId,minutes:180,cancelled:false} },
  { ...demo.dashboard.items[0].task, id:'0192f0c4-0000-7000-8000-000000000022', title:'Order sterile packs', kind:{type:'deadline'}, status:{state:'open'}, appointment:null, due:'2026-11-04T12:00:00Z', workflow:{type:'procurement',stage:'partial',supplier:'Supplier A',quoteRef:'Q-17',poRef:'PO-17',quantity:20,received:8,expected:'2026-11-05T10:00:00Z',invoiceRef:''} },
  { ...demo.dashboard.items[0].task, id:'0192f0c4-0000-7000-8000-000000000023', title:'Receive stock delivery', kind:{type:'appointment'}, status:{state:'open'}, appointment:'2026-11-05T09:00:00Z', workflow:{type:'delivery',windowEnd:'2026-11-05T12:00:00Z',receiver:'Stores role',confirmedAt:null} },
]

mockIPC((cmd, args: any) => {
  switch (cmd) {
    case 'work_data': return { tasks:workJobs, lists:demo.waitlists, templates:[{id:'0192f0c4-0000-7000-8000-000000000024',name:'Clinic setup',task:workJobs[0],archived:false,revision:1}] }
    case 'clinic_data': return { lists: demo.waitlists, entries: [clinicEntry], slots: [clinicSlot], summary: clinicSummary }
    case 'clinic_summary': return clinicSummary
    case 'clinic_candidates': return [{ entry: clinicEntry, why: 'Target 2026-11-05; same site, clinic and provider; length, type and preferred times fit.' }]
    case 'status':
      return { unlocked: mode !== 'locked', problem: mode === 'locked' ? 'The stored database key cannot be read.' : null,
        recoveryPending: mode === 'first', keyStorage: 'keyring', platform: p.get('platform') ?? 'linux',
        notice: null, setupDone: mode !== 'setup' }
    case 'recovery_key': return '3f9a-0c21-7be4-91d0-55aa-e2c7-0f13-8b6d-c4e9-27f0-a1b3-6d58-9e02-f7c4-3a16-b8d5'
    case 'get_dashboard': return demo.dashboard
    case 'focus_sessions': return []
    case 'priority_levels': return { revision: 0, levels: [{id:'low',label:'Low'},{id:'normal',label:'Normal'},{id:'high',label:'High'},{id:'veryHigh',label:'Very high'},{id:'extremelyHigh',label:'Extremely high'}] }
    case 'suggest_priority': return null
    case 'set_importance': return null
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
    case 'counters': return demo.counters
    case 'recent_counts': return []
    case 'waitlists': return demo.waitlists
    case 'get_figures': return { ...demo.figures, listId: args.listId }
    case 'preview_report': return args.req.kind === 'waitingList' ? demo.waiting : demo.performance
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
if (p.get('work')) {app.view='lists';app.clinicTab='work'}
if (p.get('workedit')) app.editing={task:structuredClone(workJobs.find(t=>t.workflow.type===p.get('workedit'))!) as any,isNew:false}
if (p.get('clinic')) {
  app.clinicTab = 'refill'
  await new Promise((r) => setTimeout(r, 300))
  document.querySelector<HTMLButtonElement>('button.slot')?.click()
}
if (p.get('edit')) {
  const t = structuredClone(demo.dashboard.items[0].task)
  t.notes = 'Offer to next on list. Was 943 476 5919 on the old sheet.'
  app.editing = { task: t as any, isNew: false }
}
if (p.get('capture')) app.capturing = true
if (p.get('report')) {
  await new Promise((r) => setTimeout(r, 300))
  if (p.get('report') === 'waiting') (document.querySelector('input[value="waitingList"]') as HTMLInputElement)?.click()
  await new Promise((r) => setTimeout(r, 100))
  ;[...document.querySelectorAll('button')].find((b) => b.textContent === 'Show report')?.click()
}
