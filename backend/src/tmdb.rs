use reqwest::Client;
use serde::Deserialize;
use anyhow::Result;

#[derive(Clone)]
pub struct TmdbClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

#[derive(Deserialize, Debug)]
struct ImageResponse {
    posters: Vec<PosterImage>,
}

#[derive(Deserialize, Debug)]
struct PosterImage {
    file_path: String,
    iso_639_1: Option<String>,
    width: u32,
    height: u32,
    vote_average: f64,
}

#[derive(Deserialize, Debug)]
struct MovieDetails {
    poster_path: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ShowDetails {
    poster_path: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize, Debug)]
struct SeasonDetails {
    poster_path: Option<String>,
}

impl TmdbClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.themoviedb.org/3".to_string(),
        }
    }

    // ==================== FILMS ====================

    /// Récupère le MEILLEUR poster textless pour un FILM (haute définition)
    pub async fn get_textless_poster(&self, tmdb_id: &str) -> Result<Option<String>> {
        let url = format!("{}/movie/{}/images?api_key={}", self.base_url, tmdb_id, self.api_key);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { return Ok(None); }

        let images: ImageResponse = resp.json().await?;

        // 1. Filtrer les candidats Textless ("xx" ou null)
        let mut candidates: Vec<&PosterImage> = images.posters.iter()
            .filter(|p| {
                match &p.iso_639_1 {
                    Some(lang) => lang == "xx" || lang == "null",
                    None => true
                }
            })
            .collect();

        // 2. Si on en a trouvé, on trie pour trouver le "King"
        if !candidates.is_empty() {
            candidates.sort_by(|a, b| {
                let res_a = a.width * a.height;
                let res_b = b.width * b.height;
                
                res_b.cmp(&res_a)
                    .then(b.vote_average.partial_cmp(&a.vote_average).unwrap_or(std::cmp::Ordering::Equal))
            });

            if let Some(best) = candidates.first() {
                println!("      ✨ Meilleur poster textless trouvé : {}x{} (Note: {})", best.width, best.height, best.vote_average);
                return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", best.file_path)));
            }
        }

        // 3. Fallback : Meilleur en Français "fr"
        let mut fr_candidates: Vec<&PosterImage> = images.posters.iter()
            .filter(|p| p.iso_639_1.as_deref() == Some("fr"))
            .collect();

        if !fr_candidates.is_empty() {
            fr_candidates.sort_by(|a, b| (b.width * b.height).cmp(&(a.width * a.height)));
            
            if let Some(best_fr) = fr_candidates.first() {
                println!("      ⚠️ Pas de textless pur, utilisation du meilleur 'fr' : {}x{}", best_fr.width, best_fr.height);
                return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", best_fr.file_path)));
            }
        }

        Ok(None)
    }

    /// Récupère le poster standard d'un FILM
    pub async fn get_standard_poster(&self, tmdb_id: &str) -> Result<Option<String>> {
        let url = format!("{}/movie/{}?api_key={}", self.base_url, tmdb_id, self.api_key);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { return Ok(None); }

        let details: MovieDetails = resp.json().await?;

        if let Some(path) = details.poster_path {
            return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", path)));
        }

        Ok(None)
    }

    // ==================== SÉRIES ====================

    /// Récupère le MEILLEUR poster textless pour une SÉRIE
    pub async fn get_show_textless_poster(&self, tmdb_id: &str) -> Result<Option<String>> {
        let url = format!("{}/tv/{}/images?api_key={}", self.base_url, tmdb_id, self.api_key);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { return Ok(None); }

        let images: ImageResponse = resp.json().await?;

        // 1. Filtrer les candidats Textless
        let mut candidates: Vec<&PosterImage> = images.posters.iter()
            .filter(|p| {
                match &p.iso_639_1 {
                    Some(lang) => lang == "xx" || lang == "null",
                    None => true
                }
            })
            .collect();

        // 2. Tri par résolution et note
        if !candidates.is_empty() {
            candidates.sort_by(|a, b| {
                let res_a = a.width * a.height;
                let res_b = b.width * b.height;
                
                res_b.cmp(&res_a)
                    .then(b.vote_average.partial_cmp(&a.vote_average).unwrap_or(std::cmp::Ordering::Equal))
            });

            if let Some(best) = candidates.first() {
                println!("      ✨ Meilleur poster textless série trouvé : {}x{} (Note: {})", best.width, best.height, best.vote_average);
                return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", best.file_path)));
            }
        }

        // 3. Fallback français
        let mut fr_candidates: Vec<&PosterImage> = images.posters.iter()
            .filter(|p| p.iso_639_1.as_deref() == Some("fr"))
            .collect();

        if !fr_candidates.is_empty() {
            fr_candidates.sort_by(|a, b| (b.width * b.height).cmp(&(a.width * a.height)));
            
            if let Some(best_fr) = fr_candidates.first() {
                println!("      ⚠️ Pas de textless série, utilisation du meilleur 'fr' : {}x{}", best_fr.width, best_fr.height);
                return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", best_fr.file_path)));
            }
        }

        Ok(None)
    }

    /// Récupère le poster standard d'une SÉRIE
    pub async fn get_show_standard_poster(&self, tmdb_id: &str) -> Result<Option<String>> {
        let url = format!("{}/tv/{}?api_key={}", self.base_url, tmdb_id, self.api_key);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { return Ok(None); }

        let details: ShowDetails = resp.json().await?;

        if let Some(path) = details.poster_path {
            return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", path)));
        }

        Ok(None)
    }

    /// Récupère le status d'une SÉRIE (Returning Series, Ended, Canceled, etc.)
    pub async fn get_show_status(&self, tmdb_id: &str) -> Result<Option<String>> {
        let url = format!("{}/tv/{}?api_key={}", self.base_url, tmdb_id, self.api_key);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { return Ok(None); }

        let details: ShowDetails = resp.json().await?;

        Ok(details.status)
    }

    // ==================== SAISONS ====================

    /// Récupère le poster d'une SAISON spécifique
    pub async fn get_season_poster(&self, show_tmdb_id: &str, season_number: u32) -> Result<Option<String>> {
        let url = format!(
            "{}/tv/{}/season/{}?api_key={}",
            self.base_url, show_tmdb_id, season_number, self.api_key
        );
        
        let resp = self.client.get(&url).send().await?;
        
        if !resp.status().is_success() {
            return Ok(None);
        }
        
        let details: SeasonDetails = resp.json().await?;
        
        if let Some(path) = details.poster_path {
            Ok(Some(format!("https://image.tmdb.org/t/p/original{}", path)))
        } else {
            Ok(None)
        }
    }

    /// Récupère le poster textless d'une SAISON (si disponible)
    pub async fn get_season_textless_poster(&self, show_tmdb_id: &str, season_number: u32) -> Result<Option<String>> {
        let url = format!(
            "{}/tv/{}/season/{}/images?api_key={}",
            self.base_url, show_tmdb_id, season_number, self.api_key
        );
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() { 
            // Fallback sur poster standard
            return self.get_season_poster(show_tmdb_id, season_number).await;
        }

        let images: ImageResponse = resp.json().await?;

        // Filtrer textless
        let mut candidates: Vec<&PosterImage> = images.posters.iter()
            .filter(|p| {
                match &p.iso_639_1 {
                    Some(lang) => lang == "xx" || lang == "null",
                    None => true
                }
            })
            .collect();

        if !candidates.is_empty() {
            candidates.sort_by(|a, b| {
                let res_a = a.width * a.height;
                let res_b = b.width * b.height;
                
                res_b.cmp(&res_a)
                    .then(b.vote_average.partial_cmp(&a.vote_average).unwrap_or(std::cmp::Ordering::Equal))
            });

            if let Some(best) = candidates.first() {
                println!("      ✨ Poster textless saison {} trouvé : {}x{}", season_number, best.width, best.height);
                return Ok(Some(format!("https://image.tmdb.org/t/p/original{}", best.file_path)));
            }
        }

        // Fallback sur poster standard si pas de textless
        self.get_season_poster(show_tmdb_id, season_number).await
    }
}
