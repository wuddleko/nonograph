use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

static REMEMBER: Mutex<()> = Mutex::new(());

const SAMPLE: &str = "\
250 OK\r\n\
250+circuit-status=\r\n\
7 BUILT $EF257C89D84EDE82C00A10896C85157A6B1E9A65~prsv,$A119B4F5FCF979E24C3F9C807F004CB38B819178~888,$D00EDE4104BB628321BEC9320C85A2CEF903FCBF~Quintex391 BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://relay.primal.net\" SOCKS_PASSWORD=\"\"\r\n\
40 BUILT $9CBBC8F41AE50BD7F45FE9BCE24DF8EB4BF10A70~datboit,$9C7354CAC7EC6BD5AF3BDBC05866606A173B610E~Valhalla,$906A5FD44849472C0F68696DC64FF08C7A623297~DFRI125 BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://relay.snort.social\" SOCKS_PASSWORD=\"\"\r\n\
.\r\n\
250 OK\r\n\
250+stream-status=\r\n\
45 SUCCEEDED 7 relay.primal.net:443\r\n\
46 SUCCEEDED 40 relay.snort.social:443\r\n\
39 NEW 0 68.67.32.34.$235396838BB8FC7AFA529042B19615DF9E2AF218.exit:9001\r\n\
.\r\n\
250 OK\r\n\
250 closing connection\r\n";

#[test]
fn circuit_status_waits_only_while_a_proxy_can_read_circuits() {
    let (tracking, relays) = circuit_status(false, None);
    assert!(!tracking);
    assert!(relays.is_empty());

    let (tracking, relays) = circuit_status(true, None);
    assert!(!tracking);
    assert!(relays.is_empty());

    let (tracking, relays) = circuit_status(true, Some(HashMap::new()));
    assert!(tracking);
    assert!(relays.is_empty());

    let live = HashMap::from([("wss://relay.example".to_string(), "a → b".to_string())]);
    let (tracking, relays) = circuit_status(true, Some(live));
    assert!(tracking);
    assert_eq!(
        relays.get("wss://relay.example").map(String::as_str),
        Some("a → b")
    );
}

#[test]
fn a_built_circuit_names_its_relay_without_a_stream() {
    let paths = paths_by_relay(SAMPLE);
    assert_eq!(
        paths.get("wss://relay.primal.net").map(String::as_str),
        Some("prsv → 888 → Quintex391")
    );
    assert_eq!(
        paths.get("wss://relay.snort.social").map(String::as_str),
        Some("datboit → Valhalla → DFRI125")
    );
    assert!(!paths.keys().any(|relay| relay.contains("exit")));
}

#[test]
fn same_host_paths_stay_apart_and_host_case_does_not() {
    let reply = "\
250+circuit-status=\r\n\
7 BUILT $AA~guard,$BB~middle,$CC~exit BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://Damus.io\"\r\n\
8 BUILT $DD~other,$EE~mid,$FF~out BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://damus.io/path/\"\r\n\
3 EXTENDED $11~early,$22~hop SOCKS_USERNAME=\"wss://offchain.pub\"\r\n\
9 BUILT $33~spare,$44~mid,$55~out BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL\r\n\
.\r\n";
    let paths = paths_by_relay(reply);
    assert_eq!(
        paths.get("wss://damus.io").map(String::as_str),
        Some("guard → middle → exit")
    );
    assert_eq!(
        paths.get("wss://damus.io/path").map(String::as_str),
        Some("other → mid → out")
    );
    assert_eq!(paths.len(), 2);
}

#[test]
fn a_gone_circuit_drops_its_path() {
    let _lock = REMEMBER.lock().expect("remember");
    let first = remember(SAMPLE);
    assert!(first.contains_key("wss://relay.primal.net"));
    let reply = "\
250+circuit-status=\r\n\
40 BUILT $9CBBC8F41AE50BD7F45FE9BCE24DF8EB4BF10A70~datboit,$9C7354CAC7EC6BD5AF3BDBC05866606A173B610E~Valhalla,$906A5FD44849472C0F68696DC64FF08C7A623297~DFRI125 BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://relay.snort.social\"\r\n\
.\r\n";
    let paths = remember(reply);
    assert_eq!(
        paths.get("wss://relay.snort.social").map(String::as_str),
        Some("datboit → Valhalla → DFRI125")
    );
    assert!(!paths.contains_key("wss://relay.primal.net"));
}

#[test]
fn a_new_circuit_keeps_the_shown_path() {
    let _lock = REMEMBER.lock().expect("remember");
    let first = remember(SAMPLE);
    let shown = first.get("wss://relay.primal.net").cloned().unwrap();
    let reply = "\
250+circuit-status=\r\n\
99 BUILT $11~newguard,$22~newmid,$33~newexit BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://relay.primal.net\"\r\n\
7 BUILT $EF257C89D84EDE82C00A10896C85157A6B1E9A65~prsv,$A119B4F5FCF979E24C3F9C807F004CB38B819178~888,$D00EDE4104BB628321BEC9320C85A2CEF903FCBF~Quintex391 BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://relay.primal.net\" SOCKS_PASSWORD=\"\"\r\n\
40 BUILT $9CBBC8F41AE50BD7F45FE9BCE24DF8EB4BF10A70~datboit,$9C7354CAC7EC6BD5AF3BDBC05866606A173B610E~Valhalla,$906A5FD44849472C0F68696DC64FF08C7A623297~DFRI125 BUILD_FLAGS=NEED_CAPACITY PURPOSE=GENERAL SOCKS_USERNAME=\"wss://relay.snort.social\" SOCKS_PASSWORD=\"\"\r\n\
.\r\n";
    let paths = remember(reply);
    assert_eq!(
        paths.get("wss://relay.primal.net").map(String::as_str),
        Some(shown.as_str())
    );
    assert_ne!(shown, "newguard → newmid → newexit");
}
