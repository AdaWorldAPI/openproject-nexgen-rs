class WorkPackage < ApplicationRecord
  belongs_to :project
  has_many :time_entries

  validates :subject, presence: true

  # Total logged hours across this work package's time entries (memoized).
  # Derived attribute -> emitted_by compute_total_hours; reads time_entries.hours.
  def compute_total_hours
    raise ActiveRecord::RecordInvalid unless status
    @total_hours ||= time_entries.hours
  end
end
