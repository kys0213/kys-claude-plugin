package architecture

import (
	"fmt"
	"strings"
	"unicode"
)

const (
	// ngramSize is the size of n-grams for similarity detection
	ngramSize = 3

	// similarityThreshold is the Jaccard similarity threshold to flag duplication
	// 0.3 means 30% of n-grams are shared — catches substantial duplication
	similarityThreshold = 0.30

	// minNgrams is the minimum number of n-grams a file must have to be compared
	// avoids false positives on very short files
	minNgrams = 20
)

// SimilarityPair represents a pair of files with detected content similarity
type SimilarityPair struct {
	FileA      string
	LayerA     Layer
	FileB      string
	LayerB     Layer
	Similarity float64
	SharedText []string // sample of shared n-grams for context
}

// validateContentSimilarity detects duplicate content scattered across layers within each plugin
func validateContentSimilarity(files []LayerFile, results *Results) {
	// Group files by plugin
	byPlugin := make(map[string][]LayerFile)
	for _, f := range files {
		byPlugin[f.Plugin] = append(byPlugin[f.Plugin], f)
	}

	for plugin, pluginFiles := range byPlugin {
		pairs := detectSimilarPairs(pluginFiles)

		if len(pairs) == 0 {
			results.Passed = append(results.Passed, Result{
				File:     fmt.Sprintf("plugins/%s", plugin),
				Type:     "content-similarity",
				Valid:    true,
				Severity: "warning",
			})
			continue
		}

		for _, pair := range pairs {
			results.Warnings = append(results.Warnings, Result{
				File:     pair.FileA,
				Type:     "content-similarity",
				Valid:    false,
				Severity: "warning",
				Errors: []string{
					fmt.Sprintf(
						"%.0f%% content overlap with %s (%s↔%s layer) — consider extracting shared content to a lower layer",
						pair.Similarity*100,
						pair.FileB,
						pair.LayerA,
						pair.LayerB,
					),
				},
			})
		}
	}
}

// detectSimilarPairs finds pairs of files across different layers with high content overlap
func detectSimilarPairs(files []LayerFile) []SimilarityPair {
	var pairs []SimilarityPair

	// Pre-compute n-gram sets for all files
	type ngramData struct {
		ngrams map[string]struct{}
		file   LayerFile
	}

	var data []ngramData
	for _, f := range files {
		ng := extractNgrams(f.Body, ngramSize)
		if len(ng) >= minNgrams {
			data = append(data, ngramData{ngrams: ng, file: f})
		}
	}

	// Compare all cross-layer pairs within the same plugin
	for i := 0; i < len(data); i++ {
		for j := i + 1; j < len(data); j++ {
			a := data[i]
			b := data[j]

			// Only compare files in DIFFERENT layers
			if a.file.Layer == b.file.Layer {
				continue
			}

			sim := jaccardSimilarity(a.ngrams, b.ngrams)
			if sim >= similarityThreshold {
				shared := sampleSharedNgrams(a.ngrams, b.ngrams, 5)
				pairs = append(pairs, SimilarityPair{
					FileA:      a.file.Path,
					LayerA:     a.file.Layer,
					FileB:      b.file.Path,
					LayerB:     b.file.Layer,
					Similarity: sim,
					SharedText: shared,
				})
			}
		}
	}

	return pairs
}

// extractNgrams builds a set of word n-grams from text, skipping code blocks and frontmatter
func extractNgrams(text string, n int) map[string]struct{} {
	ngrams := make(map[string]struct{})

	// Clean the text: remove code blocks, normalize
	cleaned := cleanForSimilarity(text)
	words := tokenize(cleaned)

	if len(words) < n {
		return ngrams
	}

	for i := 0; i <= len(words)-n; i++ {
		gram := strings.Join(words[i:i+n], " ")
		ngrams[gram] = struct{}{}
	}

	return ngrams
}

// cleanForSimilarity removes noise from text for comparison
func cleanForSimilarity(text string) string {
	lines := strings.Split(text, "\n")
	var cleaned []string
	inCodeBlock := false

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)

		// Skip code blocks
		if strings.HasPrefix(trimmed, "```") {
			inCodeBlock = !inCodeBlock
			continue
		}
		if inCodeBlock {
			continue
		}

		// Skip empty lines
		if trimmed == "" {
			continue
		}

		// Skip markdown structural elements
		if trimmed == "---" || trimmed == "===" {
			continue
		}

		// Skip pure header markers (keep the text)
		if strings.HasPrefix(trimmed, "#") {
			trimmed = strings.TrimLeft(trimmed, "# ")
		}

		// Skip list markers
		trimmed = strings.TrimPrefix(trimmed, "- ")
		trimmed = strings.TrimPrefix(trimmed, "* ")
		trimmed = strings.TrimPrefix(trimmed, "- [ ] ")
		trimmed = strings.TrimPrefix(trimmed, "- [x] ")

		// Skip very short lines (noise)
		if len(trimmed) < 10 {
			continue
		}

		cleaned = append(cleaned, strings.ToLower(trimmed))
	}

	return strings.Join(cleaned, " ")
}

// tokenize splits text into normalized word tokens
func tokenize(text string) []string {
	var words []string
	for _, word := range strings.Fields(text) {
		// Strip punctuation
		cleaned := strings.Map(func(r rune) rune {
			if unicode.IsLetter(r) || unicode.IsDigit(r) || r == '-' || r == '_' {
				return r
			}
			return -1
		}, word)

		if len(cleaned) >= 2 {
			words = append(words, cleaned)
		}
	}
	return words
}

// jaccardSimilarity computes |A∩B| / |A∪B|
func jaccardSimilarity(a, b map[string]struct{}) float64 {
	if len(a) == 0 || len(b) == 0 {
		return 0
	}

	intersection := 0
	for k := range a {
		if _, ok := b[k]; ok {
			intersection++
		}
	}

	union := len(a) + len(b) - intersection
	if union == 0 {
		return 0
	}

	return float64(intersection) / float64(union)
}

// sampleSharedNgrams returns a sample of shared n-grams for diagnostic output
func sampleSharedNgrams(a, b map[string]struct{}, maxSamples int) []string {
	var shared []string
	for k := range a {
		if _, ok := b[k]; ok {
			shared = append(shared, k)
			if len(shared) >= maxSamples {
				break
			}
		}
	}
	return shared
}
