import { MigrationInterface, QueryRunner } from 'typeorm'

export class AddBoxLifecycleSeconds1784250000000 implements MigrationInterface {
  name = 'AddBoxLifecycleSeconds1784250000000'

  public async up(queryRunner: QueryRunner): Promise<void> {
    await queryRunner.query(`ALTER TABLE "box" DROP COLUMN IF EXISTS "autoStopInterval"`)
    await queryRunner.query(`ALTER TABLE "box" DROP COLUMN IF EXISTS "autoDeleteInterval"`)
    await queryRunner.query(`ALTER TABLE "box" ADD "autoPauseInterval" integer NOT NULL DEFAULT 900`)
    await queryRunner.query(`ALTER TABLE "box" ADD "autoDeleteInterval" integer NOT NULL DEFAULT 0`)
    await queryRunner.query(`ALTER TABLE "box" ADD "autoResumeEnabled" boolean NOT NULL DEFAULT true`)
    await queryRunner.query(
      `ALTER TABLE "box" ADD CONSTRAINT "box_auto_pause_interval_nonnegative" CHECK ("autoPauseInterval" >= 0)`,
    )
    await queryRunner.query(
      `ALTER TABLE "box" ADD CONSTRAINT "box_auto_delete_interval_nonnegative" CHECK ("autoDeleteInterval" >= 0)`,
    )
  }

  public async down(queryRunner: QueryRunner): Promise<void> {
    await queryRunner.query(`ALTER TABLE "box" DROP CONSTRAINT IF EXISTS "box_auto_delete_interval_nonnegative"`)
    await queryRunner.query(`ALTER TABLE "box" DROP CONSTRAINT IF EXISTS "box_auto_pause_interval_nonnegative"`)
    await queryRunner.query(`ALTER TABLE "box" DROP COLUMN "autoResumeEnabled"`)
    await queryRunner.query(`ALTER TABLE "box" DROP COLUMN "autoDeleteInterval"`)
    await queryRunner.query(`ALTER TABLE "box" DROP COLUMN "autoPauseInterval"`)
  }
}
