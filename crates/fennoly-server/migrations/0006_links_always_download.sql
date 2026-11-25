-- Public links can no longer be "view only": a browser that can show a file
-- has already received all of it, so the switch only hid a button and gave a
-- false sense of protection. Anyone with the link may now view and download.
ALTER TABLE links DROP COLUMN allow_download;
