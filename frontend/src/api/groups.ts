import { apiFetch } from './client'

export interface Group {
  id: string
  name: string
  member_count: number
  created_at: string
}

export interface GroupDetail {
  id: string
  name: string
  members: GroupMember[]
}

export interface GroupMember {
  user_id: string
  username: string
  display_name: string
}

export function fetchGroups(): Promise<Group[]> {
  return apiFetch('/api/v1/groups')
}

export function fetchGroup(id: string): Promise<GroupDetail> {
  return apiFetch(`/api/v1/groups/${id}`)
}

export function createGroup(name: string): Promise<Group> {
  return apiFetch('/api/v1/groups', {
    method: 'POST',
    body: JSON.stringify({ name })
  })
}

export function deleteGroup(id: string): Promise<null> {
  return apiFetch(`/api/v1/groups/${id}`, { method: 'DELETE' })
}

export function addMember(groupId: string, userId: string): Promise<null> {
  return apiFetch(`/api/v1/groups/${groupId}/members`, {
    method: 'POST',
    body: JSON.stringify({ user_id: userId })
  })
}

export function removeMember(groupId: string, userId: string): Promise<null> {
  return apiFetch(`/api/v1/groups/${groupId}/members/${userId}`, {
    method: 'DELETE'
  })
}
