use super::*;
use et54::Measurements;

#[test]
fn dashboard_sync_defers_channel_changes_and_resets_history_on_reconnect() {
    let mut dashboard = super::super::Dashboard::new(String::new());
    let mut sample = Sample {
        values: Measurements {
            voltage: 12.,
            current: 1.,
            power: 12.,
            resistance: Some(12.),
        },
        when: Instant::now(),
        sequence: 1,
        channel: 1,
        epoch: 1,
    };
    let mut state = Snapshot {
        epoch: 1,
        settings_revision: 1,
        basic_setpoints: Some([1.; 4]),
        sample: Some(sample),
        ..Default::default()
    };
    dashboard.sync_from_snapshot(&state, false);
    assert_eq!(dashboard.controls.setpoints, [1.; 4]);
    sample.sequence += 1;
    state.sample = Some(sample);
    dashboard.sync_from_snapshot(&state, false);
    assert_eq!(dashboard.monitor.history.len(), 2);

    dashboard.controls.selected = 2;
    state.settings_revision += 1;
    state.basic_setpoints = Some([2.; 4]);
    dashboard.sync_from_snapshot(&state, true);
    assert_eq!(dashboard.controls.selected, 2);
    assert_eq!(dashboard.controls.setpoints, [1.; 4], "do not import another channel's settings");
    assert_eq!(dashboard.monitor.history.len(), 2, "do not record another channel's sample");
    state.busy = true;
    dashboard.sync_from_snapshot(&state, false);
    assert_eq!(dashboard.controls.selected, 2);

    state.busy = false;
    dashboard.sync_from_snapshot(&state, false);
    assert_eq!(dashboard.controls.selected, 1, "restore the confirmed channel after a failed selection");
    assert_eq!(dashboard.controls.setpoints, [2.; 4], "the deferred revision must still be imported");
    assert_eq!(dashboard.monitor.history.len(), 1);

    state.epoch += 1;
    state.selected = 2;
    state.busy = true;
    dashboard.sync_from_snapshot(&state, true);
    assert_eq!(dashboard.controls.selected, 2, "a new connection overrides pending selection");
    assert!(dashboard.monitor.history.is_empty(), "a reconnect clears old samples");
    sample.epoch = state.epoch;
    state.sample = Some(sample);
    dashboard.sync_from_snapshot(&state, true);
    assert!(dashboard.monitor.history.is_empty(), "ignore samples from the wrong channel");
    sample.channel = state.selected;
    state.sample = Some(sample);
    dashboard.sync_from_snapshot(&state, true);
    dashboard.sync_from_snapshot(&state, true);
    assert_eq!(dashboard.monitor.history.len(), 1, "record a matching sample only once");
}

#[test]
fn history_deduplicates_samples_bounds_memory_and_resets_for_a_new_channel() {
    let mut monitor = Monitor::default();
    let mut sample = Sample {
        values: Measurements {
            voltage: 12.,
            current: 1.,
            power: 12.,
            resistance: Some(12.),
        },
        when: Instant::now(),
        sequence: 0,
        channel: 1,
        epoch: 1,
    };
    for sequence in 0..2001 {
        sample.sequence = sequence;
        monitor.record_sample(sample);
        monitor.record_sample(sample);
    }
    assert_eq!(monitor.history.len(), 2000);
    assert_eq!(monitor.last_sample, Some((1, 2000, 1)));
    monitor.reset_history();
    assert!(monitor.history.is_empty());
    assert!(monitor.last_sample.is_none());
    sample.channel = 2;
    monitor.record_sample(sample);
    assert_eq!(monitor.history.len(), 1);
    assert_eq!(monitor.last_sample, Some((1, 2000, 2)));
    assert_eq!(monitor.history[0][0], 0.);
}
