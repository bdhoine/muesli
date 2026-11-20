use std::time::Instant;
use whisper_rs::{WhisperContext, WhisperContextParameters};
fn main() {
    let path = std::env::args().nth(1).unwrap();
    whisper_rs::install_logging_hooks();
    let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default()).expect("load");
    let mut st = ctx.create_state().unwrap();
    let threads = 4;
    for secs in [2.0f32, 5.0, 10.0, 30.0] {
        let pcm = vec![0.01f32; (secs * 16000.0) as usize];
        let mut times = vec![];
        for _ in 0..3 {
            let t = Instant::now();
            st.pcm_to_mel(&pcm, threads).unwrap();
            st.encode(0, threads).unwrap();
            times.push(t.elapsed().as_secs_f64());
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("{secs:4} s of audio: mel+encode {:.2} s (median of 3, {threads} threads)", times[1]);
    }
}
