UPDATE characters SET sex = 'female' WHERE sex IS NULL;
ALTER TABLE characters
    ALTER COLUMN sex SET DEFAULT 'female',
    ALTER COLUMN sex SET NOT NULL;
