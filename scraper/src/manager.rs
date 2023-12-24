use crate::types::{DomainInfo, Graph};
use rand::prelude::SliceRandom;

pub struct Manager {
    pub queue: Vec<String>,
    pub graph: Graph,
}

impl Manager {
    pub fn new(queue: Vec<String>) -> Self {
        let mut manager = Self {
            queue,
            graph: Graph::default(),
        };

        manager.read().ok();
        manager.purge();

        for (host, data) in &manager.graph.domains {
            for link in &data.links {
                let url = manager.graph.redirects.get(&link.url).unwrap_or(&link.url);
                if !manager.should_be_purged(url.clone()) && manager.should_be_queued(url.clone()) {
                    if let Ok(Ok(uri)) = url::Url::parse(host).map(|x| x.join(url)) {
                        manager.queue.push(uri.to_string());
                    }
                }
            }
        }

        manager.queue.sort();
        manager.queue.dedup();
        manager.queue.shuffle(&mut rand::thread_rng());

        manager
    }

    fn read(&mut self) -> anyhow::Result<()> {
        let text = std::fs::read_to_string("graph.json")?;
        self.graph = serde_json::from_str(&text)?;
        Ok(())
    }

    fn write(&self) -> anyhow::Result<()> {
        if std::fs::metadata("graph.bak.json").is_ok() {
            std::fs::remove_file("graph.bak.json")?;
        }

        if std::fs::metadata("graph.json").is_ok() {
            std::fs::rename("graph.json", "graph.bak.json")?;
        }

        let text = serde_json::to_string(&self.graph)?;
        std::fs::write("graph.json", text)?;

        Ok(())
    }

    pub fn dequeue(&mut self) -> Option<String> {
        let len = self.queue.len();
        if len > 0 {
            println!("queue: {}", len);
        }
        self.queue.pop()
    }

    pub fn mark_visited(&mut self, url: String) {
        let timestamp = chrono::Utc::now().timestamp() as usize;
        self.graph.visited.insert(url, timestamp);
        self.write().ok();
    }

    pub fn save(&mut self, real_url: String, info: DomainInfo) {
        if self.should_be_purged(real_url.clone()) {
            self.graph.domains.remove(&real_url);
            return;
        }

        for link in &info.links {
            if !self.graph.domains.contains_key(&link.url)
                && self.should_be_queued(link.url.clone())
            {
                if let Ok(Ok(uri)) = url::Url::parse(&real_url).map(|x| x.join(&link.url)) {
                    self.queue.push(uri.to_string());
                }
            }
        }

        self.graph.domains.insert(real_url, info);
        self.write().ok();
        self.purge();
    }

    pub fn add_redirect(&mut self, from: String, to: String) {
        self.graph.redirects.insert(from, to);
        self.write().ok();
    }

    fn purge(&mut self) {
        for (url, data) in self.graph.domains.clone() {
            if self.should_be_purged(url.clone()) {
                self.graph.domains.remove(&url);
            }

            for url in data.links {
                if self.should_be_purged(url.url.clone()) {
                    self.graph.domains.remove(&url.url);
                }
            }
        }

        self.queue.dedup();
        for entry in self.queue.clone() {
            if self.should_be_purged(entry.clone()) || !self.should_be_queued(entry.clone()) {
                self.queue.retain(|x| x != &entry);
            }
        }

        self.write().ok();
    }

    fn should_be_queued(&self, url: String) -> bool {
        let should_refetch_empty_sites = false;

        if should_refetch_empty_sites
            && self.graph.domains.contains_key(&url)
            && self.graph.domains.get(&url).unwrap().links.is_empty()
        {
            return true;
        }

        if self.graph.visited.contains_key(&url) {
            let timestamp = self.graph.visited[&url];
            let now = chrono::Utc::now().timestamp() as usize;
            let diff = now - timestamp;
            if diff < 60 * 60 * 24 * 7 {
                return false;
            }
        }

        if let Some(redirect) = self.graph.redirects.get(&url) {
            if should_refetch_empty_sites
                && self.graph.domains.contains_key(redirect)
                && self.graph.domains[redirect].links.is_empty()
            {
                return true;
            }

            if self.graph.visited.contains_key(redirect) {
                let timestamp = self.graph.visited[redirect];
                let now = chrono::Utc::now().timestamp() as usize;
                let diff = now - timestamp;
                if diff < 60 * 60 * 24 * 7 {
                    return false;
                }
            }
        }

        true
    }

    fn should_be_purged(&self, url: String) -> bool {
        if url.trim().is_empty() {
            return true;
        }

        if self.graph.domains.contains_key(&url)
            && self.graph.domains[&url].links.len() == 1
            && self.graph.domains[&url].links[0].url == url
        {
            return true;
        }

        if let Some(redirect) = self.graph.redirects.get(&url) {
            if self.graph.domains.contains_key(redirect)
                && self.graph.domains[redirect].links.len() == 1
                && self.graph.domains[redirect].links[0].url == url
            {
                return true;
            }
        }

        // oh god stop jesus christ
        if let Ok(url) = url::Url::parse(&url) {
            if url.host_str() == Some("youtube.com") {
                return true;
            }
        }

        false
    }
}
