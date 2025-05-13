use crate::{components::bgp_rib::BgpRib, live_bgp_parser::LiveBgpParserStatistics};
use std::fmt;


// Format numbers into human readable format
fn format_numbers(n: f64) -> String {
    const K: f64 = 1000.0;
    const M: f64 = K * 1000.0;
    const G: f64 = M * 1000.0;
    
    if n >= G {
        format!("{:.2} G", n / G)
    } else if n >= M {
        format!("{:.2} M", n / M)
    } else if n >= K {
        format!("{:.2} K", n / K)
    } else {
        format!("{:.2} B", n)
    }
}

// Format bytes into human readable format
fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.2} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.2} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.2} KB", bytes_f / KB)
    } else {
        format!("{}  B", bytes)
    }
}

impl fmt::Display for BgpRib {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "BGP RIB Overview:")?;
        writeln!(f, "┌──────────────┬──────────────┬──────────────┬──────────────┐")?;
        writeln!(f, "│   Router     │ IPv4 Prefixes│ IPv6 Prefixes│    Routes    │")?;
        writeln!(f, "├──────────────┼──────────────┼──────────────┼──────────────┤")?;
    
        for router in self.get_router_ids() {
            let mut id = router.to_string();
            id.truncate(8);
            let (routes, ipv4_prefixes, ipv6_prefixes) = self.get_routes_count_with_mask(&router);
    
            writeln!(f, "│ {:<12} │ {:>12} │ {:>12} │ {:>12} │", id, ipv4_prefixes, ipv6_prefixes, routes)?;
        }
    
        let (total_ipv4_prefixes, total_ipv6_prefixes) = self.get_prefix_count();
        let total_routes = self.get_total_route_count();
    
        writeln!(f, "├──────────────┼──────────────┼──────────────┼──────────────┤")?;
        writeln!(
            f,
            "│ {:<12} │ {:>12} │ {:>12} │ {:>12} │",
            "Total", total_ipv4_prefixes, total_ipv6_prefixes, total_routes
        )?;
        writeln!(f, "└──────────────┴──────────────┴──────────────┴──────────────┘")?;
        
        Ok(())
    }
}

impl fmt::Display for LiveBgpParserStatistics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let duration = self.end_time.duration_since(self.start_time);
        let messages_per_second = if duration.as_secs() > 0 {
            self.messages_processed as f64 / duration.as_secs_f64()
        } else {
            0.0
        };
        let bytes_per_second = if duration.as_secs() > 0 {
            self.bytes_received as f64 / duration.as_secs_f64()
        } else {
            0.0
        };

        writeln!(f, "LiveBGP Parser Statistics:")?;
        writeln!(f, "┌──────────────────────┬────────────────────┐")?;
        writeln!(f, "│ Metric               │ Value              │")?;
        writeln!(f, "├──────────────────────┼────────────────────┤")?;
        writeln!(f, "│ Messages Processed   │ {:>15}    │", format_numbers(self.messages_processed as f64))?;
        writeln!(f, "│ Bytes Received       │  {:>15}   │", format_bytes(self.bytes_received))?;
        writeln!(f, "│ Errors Encountered   │ {:>13}      │", self.errors_encountered)?;
        writeln!(f, "│ Messages/Second      │ {:>13.2}      │", messages_per_second)?;
        writeln!(f, "│ Bytes/Second         │   {:>14}/s │", format_bytes(bytes_per_second as u64))?;
        writeln!(f, "│ Runtime              │ {:>13.1} s    │", duration.as_secs_f64())?;
        if let Some(last_msg) = self.last_error_time {
            let time_since_last = last_msg.elapsed().as_secs_f64();
            writeln!(f, "│ Time Since Last Msg  │  {:>12.1} s    │", time_since_last)?;
        }
        writeln!(f, "└──────────────────────┴────────────────────┘")?;

        if let Some(error) = &self.last_error {
            if let Some(last_msg) = self.last_error_time {
                let error_time = last_msg.elapsed().as_secs_f64();
                writeln!(f, "\nLast Error ({:.1}s ago):", error_time)?;
                writeln!(f, "{}", error)?;
            }
        }
        Ok(())
    }
}