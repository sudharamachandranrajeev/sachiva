// Look-and-feel choices for this device. Kept in localStorage: losing them
// costs nothing, so they do not belong in the encrypted store.

export type Theme = 'system' | 'light' | 'dark'
export type Motion = 'system' | 'reduced' | 'full'

function read<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key) as T | null
    return v && allowed.includes(v) ? v : fallback
  } catch {
    return fallback
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    // Private mode or blocked storage: the choice lasts until restart.
  }
}

const systemReduced = matchMedia('(prefers-reduced-motion: reduce)')

class Prefs {
  theme = $state<Theme>(read('sachiva.theme', ['system', 'light', 'dark'], 'system'))
  motion = $state<Motion>(read('sachiva.motion', ['system', 'reduced', 'full'], 'system'))
  #systemReduced = $state(systemReduced.matches)

  constructor() {
    systemReduced.addEventListener('change', (e) => (this.#systemReduced = e.matches))
  }

  /** True when movement should be cut to the minimum. */
  get reduced(): boolean {
    return this.motion === 'reduced' || (this.motion === 'system' && this.#systemReduced)
  }

  /** Duration for a transition, 0 when motion is reduced. */
  ms(n: number): number {
    return this.reduced ? 0 : n
  }

  apply() {
    const root = document.documentElement
    if (this.theme === 'system') delete root.dataset.theme
    else root.dataset.theme = this.theme
    root.dataset.motion = this.reduced ? 'reduced' : 'full'
    write('sachiva.theme', this.theme)
    write('sachiva.motion', this.motion)
  }
}

export const prefs = new Prefs()
