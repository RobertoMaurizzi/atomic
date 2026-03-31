-- Store the atomic-server auth token so customers can retrieve it from the dashboard
ALTER TABLE instances ADD COLUMN instance_auth_token TEXT;
