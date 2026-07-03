import { MigrationInterface, QueryRunner } from 'typeorm'

export class Migration1782972000000 implements MigrationInterface {
  name = 'Migration1782972000000'

  public async up(queryRunner: QueryRunner): Promise<void> {
    await queryRunner.query(`ALTER TABLE "volume" ADD "bucketName" character varying`)
  }

  public async down(queryRunner: QueryRunner): Promise<void> {
    await queryRunner.query(`ALTER TABLE "volume" DROP COLUMN "bucketName"`)
  }
}
