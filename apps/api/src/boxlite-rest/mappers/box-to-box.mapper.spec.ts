/*
 * Copyright 2025 BoxLite AI
 * SPDX-License-Identifier: AGPL-3.0
 */

import { BoxState } from '../../box/enums/box-state.enum'
import { boxToBoxResponse, createBoxToCreateBox } from './box-to-box.mapper'

describe('BoxLite lifecycle policy mapper', () => {
  it('maps second-based create fields into the control-plane DTO', () => {
    const mapped = createBoxToCreateBox({
      auto_pause_interval: 1800,
      auto_delete_interval: 604800,
      auto_resume_enabled: false,
    })

    expect(mapped.autoPauseInterval).toBe(1800)
    expect(mapped.autoDeleteInterval).toBe(604800)
    expect(mapped.autoResumeEnabled).toBe(false)
  })

  it('returns the effective second-based policy', () => {
    const response = boxToBoxResponse({
      id: 'box-1',
      name: 'demo',
      state: BoxState.STARTED,
      labels: {},
      autoPauseInterval: 1800,
      autoDeleteInterval: 604800,
      autoResumeEnabled: false,
    } as any)

    expect(response.auto_pause_interval).toBe(1800)
    expect(response.auto_delete_interval).toBe(604800)
    expect(response.auto_resume_enabled).toBe(false)
  })

  it('defaults auto_resume_enabled to true when missing', () => {
    const response = boxToBoxResponse({
      id: 'box-1',
      name: 'demo',
      state: BoxState.STARTED,
      labels: {},
    } as any)

    expect(response.auto_resume_enabled).toBe(true)
  })
})
