# Session recovery

Desktop only, while Options > Save > Auto-backup is on.

Each running TraceDraw keeps a small marker, `session-<id>.json`, in the
auto-backup folder (`TraceDraw` in the temporary folder unless Options >
Save names another). It holds the time the session last checked in
(once a minute) and the names of the auto-backups it wrote
(`AutoBackup_of_<name>.tdraw`, every 20 minutes by default, see
`options.md`).

- Closing TraceDraw normally deletes the marker and the auto-backups that
  session wrote (unsaved drawings were already asked about).
- At start, a marker that has not checked in for 3 minutes belongs to a
  session that ended without closing (a crash, a power cut). When its
  auto-backups still exist, a question names them: Recover opens each as
  an unsaved drawing without a file (Save asks where), and deletes the
  backup that opened; Discard leaves the backups in the folder and forgets
  the marker. A marker without backups is forgotten silently.
- Two TraceDraw windows running at once each keep their own marker; one
  never offers the other's drawings while it still checks in.
- Auto-backups are written on a thread of their own from a copy of each
  drawing (pictures shared, not copied), straight to the file as the JSON
  is made, so the window keeps working while a large drawing is backed
  up. Each is written as `AutoBackup_of_<name>.tdraw.part` and renamed
  when complete, so a backup cut short never replaces a whole one. The
  status line says "Auto-backup saved" on the frame the thread is done; a
  clean exit waits for a backup still being written and then removes it
  with the others. Save (`.tdraw`) also writes straight to the file.
- Nothing is written in the browser build.

## Checks

- `recovery::tests::a_session_that_stops_checking_in_leaves_its_backups_to_recover`
- `recovery::tests::a_clean_exit_removes_the_marker_and_its_backups`
- `recovery::tests::broken_markers_never_reach_outside_the_folder`
- `app::tests::drawings_a_session_left_without_closing_are_recovered`
- `app::tests::saving_over_a_file_keeps_a_backup_and_auto_backup_saves_dirty_drawings`
- `document::tests::tdraw_json_streams_to_the_writer` (core)
- `engine::tests::a_drawing_marked_unsaved_stays_so_until_saved` (core)
