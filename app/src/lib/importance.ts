import type { Importance, PriorityConfig, Task } from './api'

export const defaultPriorities: PriorityConfig = { revision: 0, levels: [
  { id: 'low', label: 'Low' }, { id: 'normal', label: 'Normal' }, { id: 'high', label: 'High' },
  { id: 'veryHigh', label: 'Very high' }, { id: 'extremelyHigh', label: 'Extremely high' },
] }
export const priorityId = (task: Task) => task.priorityLevel ?? task.importance
export const priorityLabel = (id: string, config: PriorityConfig) => config.levels.find((p) => p.id === id)?.label ?? id
export function setTaskPriority(task: Task, id: string) {
  if (defaultPriorities.levels.some((p) => p.id === id)) {
    task.importance = id as Importance
    task.priorityLevel = null
  } else {
    task.importance = 'normal'
    task.priorityLevel = id
  }
}
