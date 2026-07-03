/*
 * Copyright 2025 Daytona Platforms Inc.
 * Modified by BoxLite AI, 2025-2026
 * SPDX-License-Identifier: AGPL-3.0
 */

import { Box } from '../entities/box.entity'
import { Runner } from '../entities/runner.entity'
import { JobType } from '../enums/job-type.enum'
import { ResourceType } from '../enums/resource-type.enum'
import { RunnerAdapterV2 } from './runnerAdapter.v2'

describe('RunnerAdapterV2 volume payloads', () => {
  function makeAdapter() {
    const jobService = { createJob: jest.fn(async () => undefined) }
    const adapter = new RunnerAdapterV2({} as any, {} as any, jobService as any)
    return { adapter, jobService }
  }

  function makeBox() {
    const box = new Box('region-1', 'box-1')
    box.image = 'boxlite/base'
    box.organizationId = 'org-1'
    box.runnerId = 'runner-1'
    box.volumes = [
      {
        volumeId: 'volume-1',
        mountPath: '/data',
        subpath: 'workspace',
        bucketName: 'boxlite-dev-volume-volume-1',
      },
    ]
    return box
  }

  it('includes bucketName in CREATE_BOX jobs', async () => {
    const { adapter, jobService } = makeAdapter()
    await adapter.init({ id: 'runner-1' } as Runner)
    const box = makeBox()

    await adapter.createBox(box)

    expect(jobService.createJob).toHaveBeenCalledWith(
      null,
      JobType.CREATE_BOX,
      'runner-1',
      ResourceType.BOX,
      box.id,
      expect.objectContaining({
        volumes: [
          {
            volumeId: 'volume-1',
            mountPath: '/data',
            subpath: 'workspace',
            bucketName: 'boxlite-dev-volume-volume-1',
          },
        ],
      }),
    )
  })

  it('includes bucketName in RECOVER_BOX jobs', async () => {
    const { adapter, jobService } = makeAdapter()
    await adapter.init({ id: 'runner-1' } as Runner)
    const box = makeBox()

    await adapter.recoverBox(box)

    expect(jobService.createJob).toHaveBeenCalledWith(
      null,
      JobType.RECOVER_BOX,
      'runner-1',
      ResourceType.BOX,
      box.id,
      expect.objectContaining({
        volumes: [
          {
            volumeId: 'volume-1',
            mountPath: '/data',
            subpath: 'workspace',
            bucketName: 'boxlite-dev-volume-volume-1',
          },
        ],
      }),
    )
  })
})
