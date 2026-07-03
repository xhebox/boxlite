/*
 * Copyright 2025 Daytona Platforms Inc.
 * Modified by BoxLite AI, 2025-2026
 * SPDX-License-Identifier: AGPL-3.0
 */

jest.mock('uuid', () => ({
  v4: jest.fn(() => 'volume-uuid'),
}))

import { NotFoundException, ServiceUnavailableException } from '@nestjs/common'
import { BadRequestError } from '../../exceptions/bad-request.exception'
import { Volume } from '../entities/volume.entity'
import { VolumeState } from '../enums/volume-state.enum'
import { VolumeService } from './volume.service'

function makeService(configValues: Record<string, unknown> = {}) {
  const volumeRepository = {
    findOne: jest.fn(),
    find: jest.fn(),
    save: jest.fn(async (volume: Volume) => volume),
  }
  const organizationService = {
    assertOrganizationIsNotSuspended: jest.fn(),
  }
  const configService = {
    get: jest.fn((key: string) => configValues[key]),
  }

  const service = new VolumeService(
    volumeRepository as any,
    {} as any,
    organizationService as any,
    configService as any,
    {} as any,
  )

  return { service, volumeRepository, organizationService, configService }
}

describe('VolumeService.create', () => {
  const organization = { id: 'org-1' } as any

  it('persists bucketName using the configured volume bucket prefix', async () => {
    const { service, volumeRepository } = makeService({
      's3.endpoint': 'http://minio:9000',
      's3.volumeBucketPrefix': 'boxlite-dev-volume-',
    })
    volumeRepository.findOne.mockResolvedValue(null)

    const volume = await service.create(organization, { name: 'data' })

    expect(volume.id).toBe('volume-uuid')
    expect(volume.name).toBe('data')
    expect(volume.bucketName).toBe('boxlite-dev-volume-volume-uuid')
    expect(volumeRepository.save).toHaveBeenCalledWith(expect.objectContaining({ bucketName: volume.bucketName }))
  })

  it('uses the legacy prefix when no volume bucket prefix is configured', async () => {
    const { service, volumeRepository } = makeService({ 's3.endpoint': 'http://minio:9000' })
    volumeRepository.findOne.mockResolvedValue(null)

    const volume = await service.create(organization, {})

    expect(volume.bucketName).toBe('boxlite-volume-volume-uuid')
  })

  it('rejects create when object storage is not configured', async () => {
    const { service } = makeService()

    await expect(service.create(organization, {})).rejects.toBeInstanceOf(ServiceUnavailableException)
  })
})

describe('VolumeService.resolveReadyVolumes', () => {
  it('returns requested volumes in request order and accepts ids or names', async () => {
    const { service, volumeRepository } = makeService()
    const byId = Object.assign(new Volume(), {
      id: 'volume-id-1',
      name: 'data',
      state: VolumeState.READY,
      bucketName: 'boxlite-dev-volume-volume-id-1',
    })
    const byName = Object.assign(new Volume(), {
      id: 'volume-id-2',
      name: 'cache',
      state: VolumeState.READY,
      bucketName: 'boxlite-dev-volume-volume-id-2',
    })
    volumeRepository.find.mockResolvedValue([byName, byId])

    await expect(service.resolveReadyVolumes('org-1', ['volume-id-1', 'cache'])).resolves.toEqual([byId, byName])
  })

  it('rejects missing and non-ready volumes', async () => {
    const { service, volumeRepository } = makeService()
    volumeRepository.find.mockResolvedValue([])

    await expect(service.resolveReadyVolumes('org-1', ['missing'])).rejects.toBeInstanceOf(NotFoundException)

    volumeRepository.find.mockResolvedValue([
      Object.assign(new Volume(), { id: 'volume-id-1', name: 'data', state: VolumeState.PENDING_CREATE }),
    ])

    await expect(service.resolveReadyVolumes('org-1', ['volume-id-1'])).rejects.toBeInstanceOf(BadRequestError)
  })
})
