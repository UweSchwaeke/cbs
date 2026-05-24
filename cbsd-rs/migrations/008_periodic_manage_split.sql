-- Copyright (C) 2026  Clyso
--
-- Audit-rem D3 / audit F4: split the legacy `periodic:manage` capability
-- into `periodic:manage:own` (manage only own tasks) and
-- `periodic:manage:any` (manage all tasks).
--
-- This migration deletes any `periodic:manage` entries from
-- `role_caps` without auto-mapping them to either new cap. Operators
-- using custom roles that previously included `periodic:manage` must
-- explicitly re-grant the desired scope after deploying this version.
-- Built-in roles (`admin`, `builder`, `viewer`) never held this cap,
-- so they are unaffected.

DELETE FROM role_caps WHERE cap = 'periodic:manage';
