/*
 * Copyright 2025 Daytona Platforms Inc.
 * Modified by BoxLite AI, 2025-2026
 * SPDX-License-Identifier: AGPL-3.0
 */

import { Box } from '../entities/box.entity'
import { BoxDto } from './box.dto'

describe('BoxDto public identity', () => {
  it('exposes a single public id without a legacy boxId alias', () => {
    const box = new Box('us', 'data-loader')
    box.organizationId = '057963b2-60ca-4356-81fc-11503e15f249'
    box.osUser = 'boxlite'

    const dto = BoxDto.fromBox(box, 'https://proxy.boxlite.dev/toolbox')

    expect(dto.id).toBe(box.id)
    expect((dto as any).boxId).toBeUndefined()
  })

  it('does not expose internal volume bucket names', () => {
    const box = new Box('us', 'data-loader')
    box.organizationId = '057963b2-60ca-4356-81fc-11503e15f249'
    box.osUser = 'boxlite'
    box.volumes = [
      {
        volumeId: 'volume-1',
        mountPath: '/data',
        subpath: 'workspace',
        bucketName: 'boxlite-dev-volume-volume-1',
      },
    ]

    const dto = BoxDto.fromBox(box, 'https://proxy.boxlite.dev/toolbox')

    expect(dto.volumes).toEqual([{ volumeId: 'volume-1', mountPath: '/data', subpath: 'workspace' }])
    expect((dto.volumes?.[0] as any).bucketName).toBeUndefined()
  })
})
